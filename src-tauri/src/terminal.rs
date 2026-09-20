use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;

use crate::{
    agent_state::{AgentStateRecord, AGENT_STATE_OPTION},
    domain::Machine,
};

pub trait TerminalRuntime {
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        root: &Path,
        executable: &Path,
        prompt: &str,
        launch: AgentLaunchContext<'_>,
    ) -> Result<String, String>;

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String>;
}

pub struct AgentLaunchContext<'a> {
    pub run_id: i64,
    pub state_file: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneSummary {
    pub pane_id: String,
    pub pane_index: u32,
    pub pid: u32,
    pub columns: u16,
    pub rows: u16,
    pub title: String,
    pub current_command: String,
    pub current_path: String,
}

pub struct TmuxControlPane {
    input: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    pane_id: String,
}

impl TmuxControlPane {
    pub fn attach<Output, Exit>(
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
        on_output: Output,
        on_agent_state: impl Fn(AgentStateRecord) + Send + 'static,
        on_exit: Exit,
    ) -> Result<Self, String>
    where
        Output: Fn(Vec<u8>) + Send + 'static,
        Exit: Fn(Option<i32>) + Send + 'static,
    {
        validate_tmux_target(session_name)?;
        validate_pane_id(pane_id)?;
        let mut child = Command::new("tmux")
            .args([
                "-C",
                "-f",
                "/dev/null",
                "-L",
                &machine.socket_name,
                "attach-session",
                "-t",
                session_name,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not attach to Pane: {error}"))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| "tmux control client did not expose stdin".to_owned())?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| "tmux control client did not expose stdout".to_owned())?;
        let child = Arc::new(Mutex::new(child));
        let reader_child = Arc::clone(&child);
        let expected_pane_id = pane_id.to_owned();
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
                        if trimmed.starts_with(b"%end ") {
                            let _ = ready_sender.send(());
                        }
                        handle_control_line(&line, &expected_pane_id, &on_output, &on_agent_state);
                    }
                    Err(_) => break,
                }
            }
            let status = reader_child
                .lock()
                .ok()
                .and_then(|mut child| child.wait().ok())
                .and_then(|status| status.code());
            on_exit(status);
        });

        let pane = Self {
            input: Arc::new(Mutex::new(input)),
            child,
            pane_id: pane_id.to_owned(),
        };
        pane.send_command("list-panes")?;
        ready_receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "tmux control client did not become ready".to_owned())?;
        pane.send_command(&format!(
            "refresh-client -B 'mission-manager-agent-state:{}:#{{{}}}'",
            pane_id, AGENT_STATE_OPTION
        ))?;
        Ok(pane)
    }

    pub fn send_input(&self, input: &[u8]) -> Result<(), String> {
        if input.is_empty() {
            return Ok(());
        }
        let encoded = input
            .iter()
            .map(|byte| format!("0x{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        self.send_command(&format!("send-keys -t {} -H {encoded}", self.pane_id))
    }

    pub fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
        if columns == 0 || rows == 0 {
            return Err("Pane dimensions must be positive".to_owned());
        }
        self.send_command(&format!("refresh-client -C {columns},{rows}"))
    }

    pub fn close(&self) -> Result<(), String> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| "tmux control client is unavailable".to_owned())?;
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            child
                .kill()
                .map_err(|error| format!("Could not close tmux control client: {error}"))?;
        }
        child
            .wait()
            .map(|_| ())
            .map_err(|error| format!("Could not reap tmux control client: {error}"))
    }

    fn send_command(&self, command: &str) -> Result<(), String> {
        let mut input = self
            .input
            .lock()
            .map_err(|_| "tmux control client is unavailable".to_owned())?;
        input
            .write_all(command.as_bytes())
            .and_then(|_| input.write_all(b"\n"))
            .and_then(|_| input.flush())
            .map_err(|error| format!("Could not send command to Pane: {error}"))
    }
}

impl Drop for TmuxControlPane {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TmuxRuntime;

impl TerminalRuntime for TmuxRuntime {
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        root: &Path,
        executable: &Path,
        prompt: &str,
        launch: AgentLaunchContext<'_>,
    ) -> Result<String, String> {
        launch_tmux_agent(machine, session_name, root, executable, prompt, launch)
    }

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String> {
        kill_tmux_session(machine, session_name)
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_tmux(machine: &Machine, args: &[String]) -> Result<String, String> {
    let output = run_tmux_output(machine, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_tmux_output(machine: &Machine, args: &[String]) -> Result<std::process::Output, String> {
    let output = Command::new("tmux")
        .args(["-f", "/dev/null", "-L"])
        .arg(&machine.socket_name)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start tmux: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            format!("tmux exited with {}", output.status)
        } else {
            detail
        })
    } else {
        Ok(output)
    }
}

pub fn list_panes(machine: &Machine, session_name: &str) -> Result<Vec<PaneSummary>, String> {
    validate_tmux_target(session_name)?;
    let output = run_tmux(
        machine,
        &[
            "list-panes".into(),
            "-t".into(),
            session_name.into(),
            "-F".into(),
            "#{pane_id}\t#{pane_index}\t#{pane_pid}\t#{pane_width}\t#{pane_height}\t#{pane_title}\t#{pane_current_command}\t#{pane_current_path}".into(),
        ],
    )?;
    output
        .lines()
        .map(parse_pane_summary)
        .collect::<Result<Vec<_>, _>>()
}

pub fn capture_pane(machine: &Machine, pane_id: &str) -> Result<Vec<u8>, String> {
    validate_pane_id(pane_id)?;
    let output = run_tmux_output(
        machine,
        &[
            "capture-pane".into(),
            "-p".into(),
            "-e".into(),
            "-t".into(),
            pane_id.into(),
        ],
    )?;
    Ok(output.stdout)
}

fn parse_pane_summary(line: &str) -> Result<PaneSummary, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() != 8 {
        return Err(format!("tmux returned an invalid Pane description: {line}"));
    }
    Ok(PaneSummary {
        pane_id: fields[0].to_owned(),
        pane_index: parse_pane_number(fields[1], "Pane index")?,
        pid: parse_pane_number(fields[2], "Pane process ID")?,
        columns: parse_pane_number(fields[3], "Pane columns")?,
        rows: parse_pane_number(fields[4], "Pane rows")?,
        title: fields[5].to_owned(),
        current_command: fields[6].to_owned(),
        current_path: fields[7].to_owned(),
    })
}

fn parse_pane_number<T>(value: &str, label: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| format!("{label} is invalid: {error}"))
}

fn validate_tmux_target(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_$@%+.,:=~-".contains(&byte))
    {
        return Err(format!(
            "tmux target contains unsupported characters: {value}"
        ));
    }
    Ok(())
}

fn validate_pane_id(value: &str) -> Result<(), String> {
    if !value.starts_with('%')
        || value.len() < 2
        || !value[1..].bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("invalid tmux Pane identity: {value}"));
    }
    Ok(())
}

fn handle_control_line(
    line: &[u8],
    pane_id: &str,
    on_output: &impl Fn(Vec<u8>),
    on_agent_state: &impl Fn(AgentStateRecord),
) {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let prefix = b"%output ";
    if !line.starts_with(prefix) {
        let prefix = b"%subscription-changed ";
        if !line.starts_with(prefix) {
            return;
        }
        let text = String::from_utf8_lossy(&line[prefix.len()..]);
        let Some((header, value)) = text.split_once(" : ") else {
            return;
        };
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 5 || fields[0] != "mission-manager-agent-state" || fields[4] != pane_id {
            return;
        }
        if let Ok(state) = serde_json::from_str::<AgentStateRecord>(value.trim()) {
            on_agent_state(state);
        }
        return;
    }
    let Some(separator) = line[prefix.len()..].iter().position(|byte| *byte == b' ') else {
        return;
    };
    let separator = prefix.len() + separator;
    if &line[prefix.len()..separator] != pane_id.as_bytes() {
        return;
    }
    on_output(decode_control_output(&line[separator + 1..]));
}

fn decode_control_output(encoded: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if index + 3 < encoded.len()
            && encoded[index] == b'\\'
            && encoded[index + 1].is_ascii_digit()
            && encoded[index + 1] <= b'7'
            && encoded[index + 2].is_ascii_digit()
            && encoded[index + 2] <= b'7'
            && encoded[index + 3].is_ascii_digit()
            && encoded[index + 3] <= b'7'
        {
            decoded.push(
                (encoded[index + 1] - b'0') * 64
                    + (encoded[index + 2] - b'0') * 8
                    + (encoded[index + 3] - b'0'),
            );
            index += 4;
        } else {
            decoded.push(encoded[index]);
            index += 1;
        }
    }
    decoded
}

fn launch_tmux_agent(
    machine: &Machine,
    session_name: &str,
    root: &Path,
    executable: &Path,
    prompt: &str,
    launch: AgentLaunchContext<'_>,
) -> Result<String, String> {
    let command = format!(
        "export AI_MISSION_MANAGER_RUN_ID={}; export AI_MISSION_MANAGER_STATE_FILE={}; export AI_MISSION_MANAGER_TMUX_PATH=tmux; export AI_MISSION_MANAGER_TMUX_SOCKET={}; export AI_MISSION_MANAGER_PANE_ID=\"$TMUX_PANE\"; exec {} {}",
        shell_quote(&launch.run_id.to_string()),
        shell_quote(&launch.state_file.to_string_lossy()),
        shell_quote(&machine.socket_name),
        shell_quote(&executable.to_string_lossy()),
        shell_quote(prompt)
    );
    let args = vec![
        "new-session".into(),
        "-d".into(),
        "-s".into(),
        session_name.into(),
        "-c".into(),
        root.to_string_lossy().into_owned(),
        "sh".into(),
        "-lc".into(),
        command,
    ];
    if let Err(error) = run_tmux(machine, &args) {
        return Err(format!("Could not start agent Pane: {error}"));
    }

    let pane_target = format!("{session_name}:0.0");
    let args = vec![
        "display-message".into(),
        "-p".into(),
        "-t".into(),
        pane_target.clone(),
        "#{pane_id}".into(),
    ];
    let pane_id = match run_tmux(machine, &args) {
        Ok(pane_id) if pane_id.starts_with('%') => pane_id,
        Ok(pane_id) => {
            let cleanup = kill_tmux_session(machine, session_name);
            return Err(format_commit_error(
                format!("tmux returned an invalid Pane identity: {pane_id}"),
                cleanup.err(),
            ));
        }
        Err(error) => {
            let cleanup = kill_tmux_session(machine, session_name);
            return Err(format_commit_error(
                format!("Could not identify the agent Pane: {error}"),
                cleanup.err(),
            ));
        }
    };

    let args = vec![
        "list-panes".into(),
        "-t".into(),
        pane_target,
        "-F".into(),
        "#{pane_id}|#{pane_dead}".into(),
    ];
    let pane_status = run_tmux(machine, &args).unwrap_or_default();
    if pane_status != format!("{pane_id}|0") {
        let cleanup = kill_tmux_session(machine, session_name);
        return Err(format_commit_error(
            "the agent Pane exited before it could be recorded".into(),
            cleanup.err(),
        ));
    }
    Ok(pane_id)
}

fn kill_tmux_session(machine: &Machine, session_name: &str) -> Result<(), String> {
    let args = vec!["kill-session".into(), "-t".into(), session_name.into()];
    run_tmux(machine, &args).map(|_| ())
}

fn format_commit_error(error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) => format!("{error}; {cleanup_error}"),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn local_agent_launch_returns_a_stable_tmux_pane_identity() {
        let directory = tempdir().expect("temporary launch directory should exist");
        let machine = Machine {
            id: 1,
            context_id: 1,
            name: "Test Mac".into(),
            socket_name: format!("ai-mission-manager-test-{}", std::process::id()),
        };
        let session_name = format!("mission-manager-test-{}", std::process::id());
        let pane_id = TmuxRuntime
            .launch_agent(
                &machine,
                &session_name,
                directory.path(),
                Path::new("/bin/sleep"),
                "30",
                AgentLaunchContext {
                    run_id: 1,
                    state_file: &directory.path().join("state.json"),
                },
            )
            .expect("tmux should launch the test process");

        let panes = run_tmux(
            &machine,
            &[
                "list-panes".into(),
                "-t".into(),
                session_name.clone(),
                "-F".into(),
                "#{pane_id}|#{pane_current_path}".into(),
            ],
        )
        .expect("the test session should remain available");
        assert!(panes.starts_with(&pane_id));
        assert!(panes.contains(directory.path().to_string_lossy().as_ref()));
        TmuxRuntime
            .kill_session(&machine, &session_name)
            .expect("the test session should be cleaned up");
    }

    #[test]
    fn control_pane_attaches_without_replacing_the_existing_process() {
        let machine = Machine {
            id: 1,
            context_id: 1,
            name: "Test Mac".into(),
            socket_name: format!("ai-mission-manager-control-test-{}", std::process::id()),
        };
        let session_name = format!("mission-manager-control-test-{}", std::process::id());
        let program = "trap 'printf INTERRUPTED\\n' INT; sleep 0.3; printf 'READY\\n'; while IFS= read -r line; do printf 'ECHO:%s\\n' \"$line\"; done";
        run_tmux(
            &machine,
            &[
                "new-session".into(),
                "-d".into(),
                "-s".into(),
                session_name.clone(),
                "sh".into(),
                "-c".into(),
                program.into(),
            ],
        )
        .expect("the existing Pane should start");

        let before = list_panes(&machine, &session_name)
            .expect("the existing Pane should be listed")
            .into_iter()
            .next()
            .expect("the test session should contain a Pane");
        let (output_sender, output_receiver) = std::sync::mpsc::channel();
        let (state_sender, state_receiver) = std::sync::mpsc::channel();
        let pane = TmuxControlPane::attach(
            &machine,
            &session_name,
            &before.pane_id,
            move |chunk| {
                output_sender
                    .send(chunk)
                    .expect("the test output receiver should remain available");
            },
            move |state| {
                state_sender
                    .send(state)
                    .expect("the test state receiver should remain available");
            },
            |_| {},
        )
        .expect("the control client should attach to the existing Pane");

        let state = serde_json::json!({
            "agent": "claude",
            "runId": "7",
            "state": "blocked",
            "updatedAt": "2026-09-19T12:34:56Z"
        })
        .to_string();
        run_tmux(
            &machine,
            &[
                "set-option".into(),
                "-p".into(),
                "-t".into(),
                before.pane_id.clone(),
                AGENT_STATE_OPTION.into(),
                state,
            ],
        )
        .expect("the test state should be published through tmux");
        assert_eq!(
            state_receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("the control client should receive the state notification")
                .state,
            crate::domain::RunState::Blocked
        );

        let ready = receive_until(&output_receiver, "READY");
        assert!(ready.contains("READY"));
        assert!(String::from_utf8_lossy(
            &capture_pane(&machine, &before.pane_id)
                .expect("the current Pane screen should be capturable")
        )
        .contains("READY"));

        pane.send_input(b"hello\n")
            .expect("input should reach the existing process");
        assert!(receive_until(&output_receiver, "ECHO:hello").contains("ECHO:hello"));
        pane.send_input(&[3])
            .expect("control input should reach the existing process");
        assert!(receive_until(&output_receiver, "INTERRUPTED").contains("INTERRUPTED"));

        pane.resize(100, 30)
            .expect("the existing Pane should be resizable");
        let after_resize = list_panes(&machine, &session_name)
            .expect("the existing Pane should remain listable")
            .into_iter()
            .find(|candidate| candidate.pane_id == before.pane_id)
            .expect("the same Pane should remain after resize");
        assert_eq!(after_resize.columns, 100);
        assert_eq!(after_resize.rows, 30);
        assert_eq!(after_resize.pid, before.pid);

        pane.close()
            .expect("closing the control client should succeed");
        let after_close = list_panes(&machine, &session_name)
            .expect("closing the view should leave the Pane alive")
            .into_iter()
            .find(|candidate| candidate.pane_id == before.pane_id)
            .expect("the original Pane should remain after detach");
        assert_eq!(after_close.pid, before.pid);
        kill_tmux_session(&machine, &session_name).expect("the test session should be cleaned up");
    }

    fn receive_until(receiver: &std::sync::mpsc::Receiver<Vec<u8>>, expected: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut output = String::new();
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if let Ok(chunk) = receiver.recv_timeout(remaining) {
                output.push_str(&String::from_utf8_lossy(&chunk));
                if output.contains(expected) {
                    return output;
                }
            }
        }
        output
    }
}
