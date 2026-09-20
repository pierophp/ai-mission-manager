use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::domain::{AgentKind, RunState};

pub const AGENT_STATE_OPTION: &str = "@ai_mission_manager_run_state";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStateRecord {
    pub agent: AgentKind,
    #[serde(rename = "runId")]
    pub run_id: String,
    pub state: RunState,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Default)]
struct AgentStateEnvironment {
    run_id: Option<OsString>,
    state_file: Option<PathBuf>,
    tmux_path: Option<OsString>,
    socket_name: Option<OsString>,
    pane_id: Option<OsString>,
}

pub fn state_for_hook_event(agent: AgentKind, event: &Value) -> Option<RunState> {
    let event_name = event.get("hook_event_name").and_then(Value::as_str)?;
    if matches!(event_name, "SessionStart" | "UserPromptSubmit") {
        return Some(RunState::Working);
    }
    if matches!(event_name, "Stop" | "SessionEnd") {
        return Some(RunState::Finished);
    }
    match agent {
        AgentKind::Claude
            if matches!(
                event.get("notification_type").and_then(Value::as_str),
                Some("permission_prompt")
                    | Some("elicitation_dialog")
                    | Some("elicitation_url_dialog")
            ) =>
        {
            Some(RunState::Blocked)
        }
        AgentKind::Codex if event_name == "PermissionRequest" => Some(RunState::Blocked),
        _ => None,
    }
}

pub fn read_state_file(path: &Path) -> Result<AgentStateRecord, String> {
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&contents).map_err(|error| error.to_string())
}

pub fn hook_command(executable: &Path, agent: AgentKind) -> String {
    format!(
        "{} --agent-state-hook {}",
        shell_quote(&executable.to_string_lossy()),
        agent_name(agent)
    )
}

pub fn provision_hooks(home: &Path, executable: &Path) -> Result<(), String> {
    provision_provider_hooks(
        &home.join(".claude/settings.json"),
        executable,
        AgentKind::Claude,
        &[
            "SessionStart",
            "UserPromptSubmit",
            "Notification",
            "Stop",
            "SessionEnd",
        ],
    )?;
    provision_provider_hooks(
        &home.join(".codex/hooks.json"),
        executable,
        AgentKind::Codex,
        &[
            "SessionStart",
            "UserPromptSubmit",
            "PermissionRequest",
            "Stop",
            "SessionEnd",
        ],
    )?;
    Ok(())
}

pub fn report_from_environment(agent: AgentKind, state: RunState) -> Result<(), String> {
    let environment = AgentStateEnvironment {
        run_id: env::var_os("AI_MISSION_MANAGER_RUN_ID"),
        state_file: env::var_os("AI_MISSION_MANAGER_STATE_FILE").map(PathBuf::from),
        tmux_path: env::var_os("AI_MISSION_MANAGER_TMUX_PATH"),
        socket_name: env::var_os("AI_MISSION_MANAGER_TMUX_SOCKET"),
        pane_id: env::var_os("AI_MISSION_MANAGER_PANE_ID"),
    };
    report_agent_state(agent, state, &environment)
}

fn report_agent_state(
    agent: AgentKind,
    state: RunState,
    environment: &AgentStateEnvironment,
) -> Result<(), String> {
    let Some(run_id) = environment.run_id.as_ref() else {
        return Ok(());
    };
    let Some(state_file) = environment.state_file.as_deref() else {
        return Ok(());
    };
    let record = AgentStateRecord {
        agent,
        run_id: run_id.to_string_lossy().into_owned(),
        state,
        updated_at: unix_timestamp(),
    };
    write_state_file(state_file, &record)?;
    let _ = notify_tmux(&record, environment);
    Ok(())
}

pub fn run_hook_from_stdin(agent_name_arg: &str) -> Result<(), String> {
    let agent = parse_agent_kind(agent_name_arg)?;
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let input = input.trim();
    if input.is_empty() {
        return Ok(());
    }
    let event: Value = match serde_json::from_str(input) {
        Ok(event) => event,
        Err(_) => return Ok(()),
    };
    if let Some(state) = state_for_hook_event(agent, &event) {
        report_from_environment(agent, state)?;
    }
    Ok(())
}

fn provision_provider_hooks(
    path: &Path,
    executable: &Path,
    agent: AgentKind,
    events: &[&str],
) -> Result<(), String> {
    let mut settings = if path.exists() {
        let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
        serde_json::from_str::<Value>(&contents)
            .map_err(|error| format!("Could not read {} as JSON: {error}", path.display()))?
    } else {
        Value::Object(Map::new())
    };
    let root = settings
        .as_object_mut()
        .ok_or_else(|| format!("{} must contain a JSON object", path.display()))?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| format!("hooks in {} must be a JSON object", path.display()))?;
    let command = hook_command(executable, agent);
    for event in events {
        let groups = hooks
            .entry((*event).to_owned())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event} in {} must be an array", path.display()))?;
        if groups
            .iter()
            .any(|group| group_contains_command(group, &command))
        {
            continue;
        }
        let mut hook = Map::new();
        hook.insert("type".into(), Value::String("command".into()));
        hook.insert("command".into(), Value::String(command.clone()));
        let mut group = Map::new();
        if *event == "Notification" {
            group.insert(
                "matcher".into(),
                Value::String("permission_prompt|elicitation_dialog|elicitation_url_dialog".into()),
            );
        }
        group.insert("hooks".into(), Value::Array(vec![Value::Object(hook)]));
        groups.push(Value::Object(group));
    }
    write_json_atomically(path, &settings)
}

fn group_contains_command(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks
                .iter()
                .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
        })
}

fn write_json_atomically(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let contents = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    file.write_all(&contents)
        .map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn write_state_file(path: &Path, record: &AgentStateRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                .map_err(|error| error.to_string())?;
        }
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let contents = serde_json::to_vec(record).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    file.write_all(&contents)
        .map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn notify_tmux(
    record: &AgentStateRecord,
    environment: &AgentStateEnvironment,
) -> Result<(), String> {
    let Some(tmux_path) = environment.tmux_path.as_ref() else {
        return Ok(());
    };
    let Some(socket_name) = environment.socket_name.as_ref() else {
        return Ok(());
    };
    let Some(pane_id) = environment.pane_id.as_ref() else {
        return Ok(());
    };
    let value = serde_json::to_string(record).map_err(|error| error.to_string())?;
    Command::new(tmux_path)
        .args(["-f", "/dev/null", "-L"])
        .arg(socket_name)
        .args(["set-option", "-p", "-t"])
        .arg(pane_id)
        .args([AGENT_STATE_OPTION, &value])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?
        .success()
        .then_some(())
        .ok_or_else(|| "tmux state notification failed".to_owned())
}

fn parse_agent_kind(value: &str) -> Result<AgentKind, String> {
    match value {
        "claude" => Ok(AgentKind::Claude),
        "codex" => Ok(AgentKind::Codex),
        _ => Err("usage: --agent-state-hook <claude|codex>".to_owned()),
    }
}

fn agent_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "claude",
        AgentKind::Codex => "codex",
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

pub fn state_file_path(directory: &Path, run_id: i64) -> PathBuf {
    directory.join(format!("run-{run_id}.json"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn hook_events_report_the_spike_mapping_without_guessing_unknown_events() {
        assert_eq!(
            state_for_hook_event(
                AgentKind::Claude,
                &serde_json::json!({
                    "hook_event_name": "SessionStart"
                })
            ),
            Some(RunState::Working)
        );
        assert_eq!(
            state_for_hook_event(
                AgentKind::Claude,
                &serde_json::json!({
                    "hook_event_name": "Notification",
                    "notification_type": "permission_prompt"
                })
            ),
            Some(RunState::Blocked)
        );
        assert_eq!(
            state_for_hook_event(
                AgentKind::Codex,
                &serde_json::json!({
                    "hook_event_name": "Notification"
                })
            ),
            None
        );
        assert_eq!(
            state_for_hook_event(
                AgentKind::Codex,
                &serde_json::json!({
                    "hook_event_name": "SessionEnd"
                })
            ),
            Some(RunState::Finished)
        );
    }

    #[test]
    fn provisioning_preserves_existing_hooks_and_is_idempotent() {
        let home = tempdir().expect("temporary home should exist");
        let settings_path = home.path().join(".claude/settings.json");
        fs::create_dir_all(settings_path.parent().expect("settings parent")).unwrap();
        fs::write(
            &settings_path,
            r#"{"permissions":{"allow":["Bash(*)"]},"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"existing"}]}]}}"#,
        )
        .unwrap();

        provision_hooks(
            home.path(),
            Path::new("/Applications/Mission Manager.app/Contents/MacOS/app"),
        )
        .expect("hooks should be provisioned");
        provision_hooks(
            home.path(),
            Path::new("/Applications/Mission Manager.app/Contents/MacOS/app"),
        )
        .expect("provisioning should be idempotent");

        let settings: Value = serde_json::from_str(
            &fs::read_to_string(settings_path).expect("settings should be readable"),
        )
        .unwrap();
        assert_eq!(settings["permissions"]["allow"][0], "Bash(*)");
        assert_eq!(
            settings["hooks"]["SessionStart"].as_array().unwrap().len(),
            2
        );
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(
            settings["hooks"]["Notification"][0]["matcher"],
            "permission_prompt|elicitation_dialog|elicitation_url_dialog"
        );
        assert_eq!(
            settings["hooks"]["Stop"][0]["hooks"][0]["command"],
            "'/Applications/Mission Manager.app/Contents/MacOS/app' --agent-state-hook claude"
        );
    }

    #[test]
    fn the_hook_writes_durable_state_even_when_tmux_notification_fails() {
        let directory = tempdir().expect("temporary state directory should exist");
        let state_file = directory.path().join("run-7.json");
        report_agent_state(
            AgentKind::Claude,
            RunState::Blocked,
            &AgentStateEnvironment {
                run_id: Some("7".into()),
                state_file: Some(state_file.clone()),
                tmux_path: Some("/definitely-not-a-tmux-executable".into()),
                socket_name: Some("mission-test".into()),
                pane_id: Some("%7".into()),
            },
        )
        .expect("a lost notification path must not fail the hook");

        let record = read_state_file(&state_file).expect("the state file should be readable");
        assert_eq!(record.run_id, "7");
        assert_eq!(record.state, RunState::Blocked);
    }
}
