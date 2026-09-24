use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::domain::{AgentKind, RunState};

pub const AGENT_STATE_OPTION: &str = "@ai_mission_manager_run_state";
const AGENT_STATE_HOOK_RELATIVE_PATH: &str =
    ".local/share/ai-mission-manager/hooks/ai-mission-manager-agent-state-hook.sh";
const AGENT_STATE_RUNS_RELATIVE_PATH: &str = ".local/state/ai-mission-manager/runs";
const AGENT_STATE_HOOK_SCRIPT: &str = r#"#!/bin/sh
umask 077

# Provider payloads are deliberately opaque; the state is fixed in argv.
cat >/dev/null || exit 1

run_id=${AI_MISSION_MANAGER_RUN_ID-}
[ -n "$run_id" ] || exit 0
case "$run_id" in *[!0-9]*) exit 0 ;; esac

state=${1-}
agent=${2-}
case "$state" in working|blocked|finished) ;; *) exit 0 ;; esac
case "$agent" in claude|codex) ;; *) exit 0 ;; esac
home=${HOME-}
[ -n "$home" ] || exit 0
state_file=${AI_MISSION_MANAGER_STATE_FILE-}
[ -n "$state_file" ] || exit 0
case "$state_file" in /*) ;; *) exit 1 ;; esac
[ "${state_file##*/}" = "run-$run_id.json" ] || exit 1
case "$state_file" in "$home"/.local/state/ai-mission-manager/runs/run-"$run_id".json) ;; *) exit 1 ;; esac

runs_dir=${state_file%/*}
app_dir=${runs_dir%/*}
state_base=${app_dir%/*}
mkdir -p "$state_base" || exit 1
for dir in "$app_dir" "$runs_dir"; do
    if [ ! -d "$dir" ]; then
        if mkdir "$dir" 2>/dev/null; then
            chmod 700 "$dir" || exit 1
        elif [ ! -d "$dir" ]; then
            exit 1
        fi
    fi
done

temporary="$state_file.tmp.$$"
updated_at=$(date +%s) || exit 1
previous_sequence=0
if [ -f "$state_file" ]; then
    previous_sequence=$(sed -n 's/.*"sequence"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\)[[:space:]]*[,}].*/\1/p' \
        "$state_file" 2>/dev/null | head -n 1)
    case "$previous_sequence" in
        ''|*[!0-9]*) previous_sequence=0 ;;
    esac
fi
sequence=$((previous_sequence + 1))
record=$(printf '{"agent":"%s","runId":"%s","state":"%s","updatedAt":"%s","sequence":%s}' \
    "$agent" "$run_id" "$state" "$updated_at" "$sequence") || exit 1

(umask 077; set -C; printf '%s\n' "$record" >"$temporary") || exit 1
chmod 600 "$temporary" || { rm -f "$temporary"; exit 1; }
if ! mv -f "$temporary" "$state_file"; then
    rm -f "$temporary"
    exit 1
fi

tmux_path=${AI_MISSION_MANAGER_TMUX_PATH-}
socket_name=${AI_MISSION_MANAGER_TMUX_SOCKET-}
pane_id=${AI_MISSION_MANAGER_PANE_ID-}
if [ -n "$tmux_path" ] && [ -n "$socket_name" ] && [ -n "$pane_id" ]; then
    "$tmux_path" -f /dev/null -L "$socket_name" set-option -p -t "$pane_id" \
        @ai_mission_manager_run_state "$record" >/dev/null 2>&1 || :
fi
exit 0
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStateRecord {
    pub agent: AgentKind,
    #[serde(rename = "runId")]
    pub run_id: String,
    pub state: RunState,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
}

#[cfg(test)]
fn state_for_hook_event(agent: AgentKind, event: &Value) -> Option<RunState> {
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

pub fn state_runs_directory(home: &Path) -> PathBuf {
    home.join(AGENT_STATE_RUNS_RELATIVE_PATH)
}

pub fn install_agent_state_hook(home: &Path) -> Result<PathBuf, String> {
    let path = home.join(AGENT_STATE_HOOK_RELATIVE_PATH);
    let hooks_dir = path
        .parent()
        .ok_or_else(|| "agent state hook path has no parent directory".to_owned())?;
    create_owned_directories(hooks_dir)?;
    let installed = fs::read(&path).ok().is_some_and(|contents| {
        contents == AGENT_STATE_HOOK_SCRIPT.as_bytes() && is_executable(&path)
    });
    if !installed {
        write_executable_atomically(&path, AGENT_STATE_HOOK_SCRIPT.as_bytes())?;
    }
    Ok(path)
}

pub fn provision_hooks(home: &Path) -> Result<(), String> {
    let script = install_agent_state_hook(home)?;
    provision_provider_hooks(
        &home.join(".claude/settings.json"),
        &script,
        AgentKind::Claude,
        &[
            ("SessionStart", RunState::Working),
            ("UserPromptSubmit", RunState::Working),
            ("Notification", RunState::Blocked),
            ("Stop", RunState::Finished),
            ("SessionEnd", RunState::Finished),
        ],
    )?;
    provision_provider_hooks(
        &home.join(".codex/hooks.json"),
        &script,
        AgentKind::Codex,
        &[
            ("SessionStart", RunState::Working),
            ("UserPromptSubmit", RunState::Working),
            ("PermissionRequest", RunState::Blocked),
            ("Stop", RunState::Finished),
            ("SessionEnd", RunState::Finished),
        ],
    )?;
    Ok(())
}

fn provision_provider_hooks(
    path: &Path,
    script: &Path,
    agent: AgentKind,
    events: &[(&str, RunState)],
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
    for groups_value in hooks.values_mut() {
        let Some(groups) = groups_value.as_array_mut() else {
            continue;
        };
        groups.retain_mut(|group| {
            let Some(group_hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = group_hooks.len();
            group_hooks.retain(|hook| {
                !hook
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_app_agent_state_command)
            });
            let removed_our_hook = group_hooks.len() != before;
            !(removed_our_hook && group_hooks.is_empty())
        });
    }
    for (event, state) in events {
        let groups = hooks
            .entry((*event).to_owned())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event} in {} must be an array", path.display()))?;
        let mut hook = Map::new();
        hook.insert("type".into(), Value::String("command".into()));
        hook.insert(
            "command".into(),
            Value::String(hook_command(script, agent, *state)),
        );
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

fn hook_command(script: &Path, agent: AgentKind, state: RunState) -> String {
    format!(
        "{} {} {}",
        shell_quote(&script.to_string_lossy()),
        run_state_name(state),
        agent_name(agent)
    )
}

fn is_app_agent_state_command(command: &str) -> bool {
    command.contains("--agent-state-hook")
        || command.contains(".local/share/ai-mission-manager/")
}

fn run_state_name(state: RunState) -> &'static str {
    match state {
        RunState::Unknown => "unknown",
        RunState::Working => "working",
        RunState::Blocked => "blocked",
        RunState::Finished => "finished",
    }
}

fn create_owned_directories(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    let mut app_owned = false;
    for component in path.components() {
        current.push(component.as_os_str());
        app_owned |= component.as_os_str() == "ai-mission-manager";
        if current.is_dir() {
            continue;
        }
        match fs::create_dir(&current) {
            Ok(()) if app_owned => set_permissions(&current, 0o700)?,
            Ok(()) => {}
            Err(_error) if current.is_dir() => {}
            Err(error) => return Err(format!("Could not create {}: {error}", current.display())),
        }
    }
    Ok(())
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

fn set_permissions(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn write_executable_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("sh.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        set_permissions(&temporary, 0o700)?;
        file.write_all(contents)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
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

fn agent_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "claude",
        AgentKind::Codex => "codex",
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn state_file_path(directory: &Path, run_id: i64) -> PathBuf {
    directory.join(format!("run-{run_id}.json"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };

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
            r#"{"permissions":{"allow":["Bash(*)"]},"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"existing"}]},{"hooks":[{"type":"command","command":"'/old/app' --agent-state-hook claude"}]},{"hooks":[{"type":"command","command":"'/old/.local/share/ai-mission-manager/ai-mission-manager-agent-state-hook.sh' blocked claude"}]}],"UserPromptSubmit":[{"hooks":[{"type":"command","command":"'~/bin/report-agent-state.sh'"}]}],"Stop":[{"hooks":[{"type":"command","command":"'/old/.local/share/ai-mission-manager/report-agent-state.sh' finished claude"}]}]}}"#,
        )
        .unwrap();
        let codex_settings_path = home.path().join(".codex/hooks.json");
        fs::create_dir_all(codex_settings_path.parent().unwrap()).unwrap();
        fs::write(
            &codex_settings_path,
            r#"{"model":"gpt-5","hooks":{"PermissionRequest":[{"hooks":[{"type":"command","command":"user-codex-hook"}]},{"hooks":[{"type":"command","command":"'/old/app' --agent-state-hook codex"}]}]}}"#,
        )
        .unwrap();

        provision_hooks(home.path()).expect("hooks should be provisioned");
        let first_claude_settings = fs::read(&settings_path).unwrap();
        provision_hooks(home.path()).expect("provisioning should be idempotent");
        assert_eq!(fs::read(&settings_path).unwrap(), first_claude_settings);

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
            hook_command(
                &home.path().join(AGENT_STATE_HOOK_RELATIVE_PATH),
                AgentKind::Claude,
                RunState::Finished
            )
        );
        assert_eq!(
            settings["hooks"]["Notification"][0]["hooks"][0]["command"],
            hook_command(
                &home.path().join(AGENT_STATE_HOOK_RELATIVE_PATH),
                AgentKind::Claude,
                RunState::Blocked
            )
        );
        assert_eq!(
            settings["hooks"]["SessionStart"][1]["hooks"][0]["command"],
            hook_command(
                &home.path().join(AGENT_STATE_HOOK_RELATIVE_PATH),
                AgentKind::Claude,
                RunState::Working
            )
        );
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "'~/bin/report-agent-state.sh'"
        );
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert!(is_executable(
            &home.path().join(AGENT_STATE_HOOK_RELATIVE_PATH)
        ));
        let codex_settings: Value =
            serde_json::from_slice(&fs::read(codex_settings_path).unwrap()).unwrap();
        assert_eq!(codex_settings["model"], "gpt-5");
        assert_eq!(
            codex_settings["hooks"]["PermissionRequest"][0]["hooks"][0]["command"],
            "user-codex-hook"
        );
        assert_eq!(
            codex_settings["hooks"]["PermissionRequest"][1]["hooks"][0]["command"],
            hook_command(
                &home.path().join(AGENT_STATE_HOOK_RELATIVE_PATH),
                AgentKind::Codex,
                RunState::Blocked
            )
        );
    }

    #[test]
    fn installed_sh_hook_writes_private_state_and_preserves_parent_permissions() {
        let home = tempdir().expect("temporary home should exist");
        let xdg_state = home.path().join(".local/state");
        fs::create_dir_all(&xdg_state).unwrap();
        fs::set_permissions(&xdg_state, fs::Permissions::from_mode(0o751)).unwrap();

        let script = install_agent_state_hook(home.path()).expect("hook should install");
        let mut child = Command::new("sh")
            .arg(script)
            .args(["working", "claude"])
            .env("HOME", home.path())
            .env("AI_MISSION_MANAGER_RUN_ID", "7")
            .env(
                "AI_MISSION_MANAGER_STATE_FILE",
                state_file_path(&state_runs_directory(home.path()), 7),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the system sh should run the hook");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"not parsed by the hook")
            .unwrap();
        assert!(child.wait().unwrap().success());

        let state_file =
            state_file_path(&home.path().join(".local/state/ai-mission-manager/runs"), 7);
        let record = read_state_file(&state_file).expect("the state record should be readable");
        assert_eq!(record.agent, AgentKind::Claude);
        assert_eq!(record.run_id, "7");
        assert_eq!(record.state, RunState::Working);
        assert_eq!(record.sequence, Some(1));
        assert_eq!(
            fs::metadata(&state_file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(state_file.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(state_file.parent().unwrap().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&xdg_state).unwrap().permissions().mode() & 0o777,
            0o751
        );
    }

    #[test]
    fn installed_sh_hook_reports_working_blocked_and_finished_states() {
        let home = tempdir().expect("temporary home should exist");
        let script = install_agent_state_hook(home.path()).expect("hook should install");

        for (run_id, state) in [(1, "working"), (2, "blocked"), (3, "finished")] {
            let status =
                run_installed_hook(&script, home.path(), state, Some(&run_id.to_string()), None);
            assert!(status.success(), "{state} should be reported");
            let state_file = state_file_path(&state_runs_directory(home.path()), run_id);
            let record = read_state_file(&state_file).expect("the state should be readable");
            assert_eq!(record.sequence, Some(1));
            assert_eq!(
                record.state,
                match state {
                    "working" => RunState::Working,
                    "blocked" => RunState::Blocked,
                    "finished" => RunState::Finished,
                    _ => unreachable!(),
                }
            );
        }
    }

    #[test]
    fn installed_sh_hook_increments_the_existing_record_sequence() {
        let home = tempdir().expect("temporary home should exist");
        let script = install_agent_state_hook(home.path()).expect("hook should install");
        let state_file = state_file_path(&state_runs_directory(home.path()), 7);

        for expected_sequence in 1..=3 {
            let status = run_installed_hook(&script, home.path(), "working", Some("7"), None);
            assert!(status.success(), "the hook should report each event");
            let record = read_state_file(&state_file).expect("the state should be readable");
            assert_eq!(record.sequence, Some(expected_sequence));
        }
    }

    #[test]
    fn older_agent_state_records_deserialize_without_a_sequence() {
        let record: AgentStateRecord = serde_json::from_str(
            r#"{"agent":"claude","runId":"7","state":"working","updatedAt":"123"}"#,
        )
        .expect("an older state record should remain readable");

        assert_eq!(record.sequence, None);
    }

    #[test]
    fn installed_sh_hook_leaves_previous_record_intact_when_atomic_replace_fails() {
        let home = tempdir().expect("temporary home should exist");
        let script = install_agent_state_hook(home.path()).expect("hook should install");
        let state_dir = home.path().join(".local/state/ai-mission-manager/runs");
        let state_file = state_file_path(&state_dir, 7);
        run_installed_hook(&script, home.path(), "working", Some("7"), None);
        let previous = fs::read(&state_file).expect("first state should be written");

        let fake_bin = home.path().join("fake-bin");
        fs::create_dir_all(&fake_bin).unwrap();
        let failing_mv = fake_bin.join("mv");
        fs::write(&failing_mv, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&failing_mv, fs::Permissions::from_mode(0o700)).unwrap();
        let result =
            run_installed_hook(&script, home.path(), "blocked", Some("7"), Some(&fake_bin));

        assert!(!result.success());
        assert_eq!(fs::read(&state_file).unwrap(), previous);
    }

    #[test]
    fn installed_sh_hook_drains_input_and_exits_without_run_environment() {
        let home = tempdir().expect("temporary home should exist");
        let script = install_agent_state_hook(home.path()).expect("hook should install");
        let result = Command::new("sh")
            .arg(script)
            .args(["working", "claude"])
            .env("HOME", home.path())
            .env_remove("AI_MISSION_MANAGER_RUN_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|mut child| {
                child.stdin.take().unwrap().write_all(b"{").unwrap();
                child.wait().unwrap()
            })
            .expect("the system sh should run the hook");

        assert!(result.success());
        assert!(!home.path().join(".local/state").exists());
    }

    fn run_installed_hook(
        script: &Path,
        home: &Path,
        state: &str,
        run_id: Option<&str>,
        path_prefix: Option<&Path>,
    ) -> std::process::ExitStatus {
        let mut command = Command::new("sh");
        command
            .arg(script)
            .args([state, "claude"])
            .env("HOME", home)
            .env(
                "AI_MISSION_MANAGER_STATE_FILE",
                state_file_path(
                    &state_runs_directory(home),
                    run_id.unwrap_or("7").parse().unwrap_or(7),
                ),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(run_id) = run_id {
            command.env("AI_MISSION_MANAGER_RUN_ID", run_id);
        } else {
            command.env_remove("AI_MISSION_MANAGER_RUN_ID");
        }
        if let Some(path_prefix) = path_prefix {
            command.env("PATH", format!("{}:/usr/bin:/bin", path_prefix.display()));
        }
        let mut child = command.spawn().expect("the system sh should run the hook");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"ignored input")
            .unwrap();
        child.wait().unwrap()
    }

    #[test]
    fn installed_sh_hook_succeeds_when_tmux_notification_fails() {
        let home = tempdir().expect("temporary home should exist");
        let script = install_agent_state_hook(home.path()).expect("hook should install");
        let state_file = state_file_path(&state_runs_directory(home.path()), 7);
        let status = Command::new("sh")
            .arg(script)
            .args(["blocked", "claude"])
            .env("HOME", home.path())
            .env("AI_MISSION_MANAGER_RUN_ID", "7")
            .env("AI_MISSION_MANAGER_STATE_FILE", &state_file)
            .env("AI_MISSION_MANAGER_TMUX_PATH", "/definitely/not/tmux")
            .env("AI_MISSION_MANAGER_TMUX_SOCKET", "mission-test")
            .env("AI_MISSION_MANAGER_PANE_ID", "%7")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|mut child| {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(b"opaque payload")
                    .unwrap();
                child.wait().unwrap()
            })
            .expect("the system sh should run the hook");

        assert!(status.success());
        let record = read_state_file(&state_file).expect("the state file should be readable");
        assert_eq!(record.run_id, "7");
        assert_eq!(record.state, RunState::Blocked);
    }
}
