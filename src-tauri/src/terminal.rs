use std::{
    collections::VecDeque,
    env,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
use std::sync::Condvar;

use serde::Serialize;

use crate::{
    agent_state::{
        self, AgentStateRecord, AGENT_STATE_HOOK_RELATIVE_PATH, AGENT_STATE_OPTION,
        AGENT_STATE_RUNS_RELATIVE_PATH,
    },
    domain::{AgentKind, Machine, MachineTransport},
};

pub trait TerminalRuntime: Send + Sync {
    fn preflight_agent_run(
        &self,
        machine: &Machine,
        agent: AgentKind,
        run_id: i64,
        preferred_executable: Option<&Path>,
    ) -> MachineRunPreflight;

    fn check_machine(&self, machine: &Machine) -> MachineReadiness;

    fn observe_machine(&self, machine: &Machine, state_run_ids: &[i64]) -> ObservedMachine;

    fn capture_pane_transcript(&self, machine: &Machine, pane_id: &str) -> Result<Vec<u8>, String>;

    fn list_panes(&self, machine: &Machine, session_name: &str)
        -> Result<Vec<PaneSummary>, String>;

    fn list_agent_panes(&self, machine: &Machine) -> Result<Vec<AgentPaneSummary>, String>;

    fn send_pane_input(
        &self,
        machine: &Machine,
        pane_id: &str,
        input: &[u8],
    ) -> Result<(), String> {
        send_input_to_pane(machine, pane_id, input)
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        gate_channel: &str,
        root: &Path,
        executable: &Path,
        prompt: &str,
        launch: AgentLaunchContext<'_>,
    ) -> Result<String, String>;

    fn release_agent_launch(&self, machine: &Machine, gate_channel: &str) -> Result<(), String>;

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String>;

    fn kill_pane(&self, machine: &Machine, session_name: &str, pane_id: &str)
        -> Result<(), String>;

    fn interrupt_pane(
        &self,
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
    ) -> Result<(), String>;
}

pub struct AgentLaunchContext<'a> {
    pub run_id: i64,
    pub state_file: &'a Path,
    pub agent: AgentKind,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPaneSummary {
    pub agent: AgentKind,
    pub session_name: String,
    pub pane_id: String,
    pub current_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPane {
    pub session_name: String,
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedMachine {
    pub panes: Result<Vec<ObservedPane>, MachineObservationError>,
    pub pane_state_records: Vec<ObservedPaneAgentState>,
    pub state_file_records: Vec<AgentStateRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPaneAgentState {
    pub session_name: String,
    pub pane_id: String,
    pub record: AgentStateRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MachineObservationFailureKind {
    Unreachable,
    TmuxQueryFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineObservationError {
    pub kind: MachineObservationFailureKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineObservationFailure {
    pub machine_id: i64,
    pub machine_name: String,
    pub kind: MachineObservationFailureKind,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReconciliationResult {
    pub failures: Vec<MachineObservationFailure>,
    pub changed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHookReadiness {
    pub provisioned: Option<bool>,
    pub current: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineReadiness {
    pub reachable: Option<bool>,
    pub tmux_available: Option<bool>,
    pub claude_executable_resolved: Option<bool>,
    pub codex_executable_resolved: Option<bool>,
    pub state_directory_writable: Option<bool>,
    pub claude_hooks: AgentHookReadiness,
    pub codex_hooks: AgentHookReadiness,
    pub last_provisioning_error: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineRunPreflight {
    pub readiness: MachineReadiness,
    pub executable: Option<PathBuf>,
    pub state_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTransport {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub known_hosts_file: Option<String>,
    pub strict_host_key_checking: Option<String>,
    pub ssh_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalTransport {
    Local,
    Ssh(SshTransport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalPaneIdentity {
    pub tmux_path: String,
    pub socket_name: String,
    pub session_name: String,
    pub pane_id: String,
    pub transport: TerminalTransport,
}

/// Builds the command that a real terminal client executes for one stored Pane.
///
/// The session lookup happens before attach so a missing or stale identity cannot
/// fall through to the session's current/default Pane.
pub fn build_pane_attach_command(identity: &ExternalPaneIdentity) -> Result<String, String> {
    if identity.tmux_path.trim().is_empty() {
        return Err("tmux executable path cannot be blank".to_owned());
    }
    validate_tmux_target(&identity.socket_name)?;
    validate_tmux_target(&identity.session_name)?;
    validate_pane_id(&identity.pane_id)?;

    let lookup = tmux_command(
        identity,
        &[
            "display-message",
            "-p",
            "-t",
            &identity.pane_id,
            "#{session_name}",
        ],
    );
    let attach = tmux_command(identity, &["attach-session", "-t", &identity.pane_id]);
    let expected_session = shell_quote(&identity.session_name);
    let missing_message = shell_quote(&format!(
        "Pane {} in session {} was not found",
        identity.pane_id, identity.session_name
    ));
    let wrong_session_message = shell_quote(&format!(
        "Pane {} is not in stored session {}",
        identity.pane_id, identity.session_name
    ));
    let remote_command = format!(
        "actual_session=\"$({lookup} 2>/dev/null)\"; if [ -z \"$actual_session\" ]; then printf '%s\\n' {missing_message}; exit 1; fi; if [ \"$actual_session\" != {expected_session} ]; then printf '%s\\n' {wrong_session_message}; exit 1; fi; exec {attach}"
    );

    match &identity.transport {
        TerminalTransport::Local => Ok(remote_command),
        TerminalTransport::Ssh(transport) => {
            validate_ssh_transport(transport)?;
            let target = match &transport.user {
                Some(user) => format!("{user}@{}", transport.host),
                None => transport.host.clone(),
            };
            let mut args = vec![
                transport.ssh_path.as_deref().unwrap_or("ssh").to_owned(),
                "-tt".to_owned(),
                "-o".to_owned(),
                "BatchMode=yes".to_owned(),
            ];
            if let Some(port) = transport.port {
                args.extend(["-p".to_owned(), port.to_string()]);
            }
            if let Some(identity_file) = &transport.identity_file {
                args.extend(["-i".to_owned(), identity_file.clone()]);
            }
            if let Some(known_hosts_file) = &transport.known_hosts_file {
                args.extend([
                    "-o".to_owned(),
                    format!("UserKnownHostsFile={known_hosts_file}"),
                ]);
            }
            if let Some(strict_host_key_checking) = &transport.strict_host_key_checking {
                args.extend([
                    "-o".to_owned(),
                    format!("StrictHostKeyChecking={strict_host_key_checking}"),
                ]);
            }
            args.extend([target, remote_command]);
            Ok(args
                .iter()
                .map(|argument| shell_quote(argument))
                .collect::<Vec<_>>()
                .join(" "))
        }
    }
}

pub fn open_pane_in_terminal(identity: &ExternalPaneIdentity) -> Result<(), String> {
    let command = build_pane_attach_command(identity)?;
    let script = build_terminal_apple_script(&command, "Terminal");
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|error| format!("Could not open macOS Terminal: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if detail.is_empty() {
        format!("macOS Terminal exited with {}", output.status)
    } else {
        format!("Could not open macOS Terminal: {detail}")
    })
}

fn tmux_command(identity: &ExternalPaneIdentity, args: &[&str]) -> String {
    let mut command = vec![
        identity.tmux_path.as_str(),
        "-f",
        "/dev/null",
        "-L",
        identity.socket_name.as_str(),
    ];
    command.extend(args.iter().copied());
    command
        .iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_ssh_transport(transport: &SshTransport) -> Result<(), String> {
    if transport.host.is_empty()
        || !transport
            .host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".@:_-".contains(&byte))
    {
        return Err(format!(
            "SSH host contains unsupported characters: {}",
            transport.host
        ));
    }
    if let Some(user) = &transport.user {
        if user.is_empty()
            || !user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(format!("SSH user contains unsupported characters: {user}"));
        }
    }
    if let Some(port) = transport.port {
        if port == 0 {
            return Err("SSH port must be positive".to_owned());
        }
    }
    if let Some(strict_host_key_checking) = &transport.strict_host_key_checking {
        if !matches!(
            strict_host_key_checking.as_str(),
            "yes" | "accept-new" | "no"
        ) {
            return Err(format!(
                "unsupported SSH StrictHostKeyChecking value: {strict_host_key_checking}"
            ));
        }
    }
    Ok(())
}

fn build_terminal_apple_script(command: &str, terminal_app: &str) -> String {
    format!(
        "tell application {}\nactivate\ndo script {}\nend tell",
        apple_script_string(terminal_app),
        apple_script_string(command)
    )
}

fn apple_script_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

pub struct TmuxControlPane {
    input: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    pending_responses: Arc<Mutex<VecDeque<PendingControlCommand>>>,
    next_response_id: AtomicU64,
    operation: Mutex<bool>,
    pane_id: String,
}

struct PendingControlCommand {
    id: u64,
    response_sender: Option<mpsc::SyncSender<Result<Vec<u8>, String>>>,
    after_end: Option<Box<dyn FnOnce() + Send>>,
}

struct TmuxControlReader<Output, AgentState> {
    expected_pane_id: String,
    pending_responses: Arc<Mutex<VecDeque<PendingControlCommand>>>,
    current_response: Option<PendingControlCommand>,
    current_response_header: Vec<u8>,
    response_body: Vec<u8>,
    ready_sender: mpsc::SyncSender<()>,
    on_output: Output,
    on_agent_state: AgentState,
}

impl<Output, AgentState> TmuxControlReader<Output, AgentState>
where
    Output: Fn(Vec<u8>),
    AgentState: Fn(AgentStateRecord),
{
    fn new(
        expected_pane_id: String,
        pending_responses: Arc<Mutex<VecDeque<PendingControlCommand>>>,
        ready_sender: mpsc::SyncSender<()>,
        on_output: Output,
        on_agent_state: AgentState,
    ) -> Self {
        Self {
            expected_pane_id,
            pending_responses,
            current_response: None,
            current_response_header: Vec::new(),
            response_body: Vec::new(),
            ready_sender,
            on_output,
            on_agent_state,
        }
    }

    fn handle_line(&mut self, line: &[u8]) {
        let without_newline = line.strip_suffix(b"\n").unwrap_or(line);
        let trimmed = without_newline
            .strip_suffix(b"\r")
            .unwrap_or(without_newline);

        if self.current_response.is_none() {
            if let Some(header) = trimmed.strip_prefix(b"%begin ") {
                self.current_response_header = header.to_vec();
                self.current_response = Some(
                    self.pending_responses
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .pop_front()
                        .unwrap_or(PendingControlCommand {
                            id: 0,
                            response_sender: None,
                            after_end: None,
                        }),
                );
                self.response_body.clear();
                return;
            }
            handle_control_line(
                line,
                &self.expected_pane_id,
                &self.on_output,
                &self.on_agent_state,
            );
            return;
        }

        if let Some(header) = trimmed.strip_prefix(b"%end ") {
            if header == self.current_response_header {
                let mut response = self
                    .current_response
                    .take()
                    .expect("a control response block should have a pending command");
                if let Some(after_end) = response.after_end.take() {
                    after_end();
                }
                if let Some(sender) = response.response_sender.take() {
                    let _ = sender.send(Ok(std::mem::take(&mut self.response_body)));
                }
                self.current_response_header.clear();
                self.response_body.clear();
                let _ = self.ready_sender.try_send(());
                return;
            }
        }
        if let Some(header) = trimmed.strip_prefix(b"%error ") {
            if header == self.current_response_header {
                let mut response = self
                    .current_response
                    .take()
                    .expect("a control response block should have a pending command");
                let error = String::from_utf8_lossy(&self.response_body)
                    .trim()
                    .to_owned();
                if let Some(sender) = response.response_sender.take() {
                    let _ = sender.send(Err(if error.is_empty() {
                        "tmux control command failed".to_owned()
                    } else {
                        error
                    }));
                }
                self.current_response_header.clear();
                self.response_body.clear();
                let _ = self.ready_sender.try_send(());
                return;
            }
        }

        self.response_body.extend_from_slice(line);
    }

    fn finish(&mut self) {
        if let Some(mut response) = self.current_response.take() {
            if let Some(sender) = response.response_sender.take() {
                let _ = sender.send(Err("tmux control client closed during a command".into()));
            }
        }
        for mut response in self
            .pending_responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
        {
            if let Some(sender) = response.response_sender.take() {
                let _ = sender.send(Err(
                    "tmux control client closed before a command completed".into()
                ));
            }
        }
    }
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
        let transport = terminal_transport(machine);
        let tmux_args = [
            "-C",
            "-f",
            "/dev/null",
            "-L",
            &machine.socket_name,
            "attach-session",
            "-t",
            session_name,
        ];
        let (program, arguments) =
            build_tmux_process_command(&transport, false, &tmux_args, "tmux")?;
        let mut child = Command::new(program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "Could not attach to Pane on Machine {}: {error}",
                    machine.name
                )
            })?;
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
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let pending_responses = Arc::new(Mutex::new(VecDeque::new()));
        let mut control_reader = TmuxControlReader::new(
            expected_pane_id,
            Arc::clone(&pending_responses),
            ready_sender,
            on_output,
            on_agent_state,
        );
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        control_reader.handle_line(&line);
                    }
                    Err(_) => break,
                }
            }
            control_reader.finish();
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
            pending_responses,
            next_response_id: AtomicU64::new(0),
            operation: Mutex::new(false),
            pane_id: pane_id.to_owned(),
        };
        pane.send_command("list-panes")?;
        ready_receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| {
                format!(
                    "Could not connect to Machine {}: tmux control client did not become ready",
                    machine.name
                )
            })?;
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

    /// Captures the visible Pane screen as bytes ready to replay into a fresh
    /// terminal emulator, including the cursor position.
    pub fn capture_pane_snapshot(
        &self,
        after_end: impl FnOnce() + Send + 'static,
    ) -> Result<Vec<u8>, String> {
        validate_pane_id(&self.pane_id)?;
        let (screen_sender, screen_receiver) = mpsc::sync_channel(1);
        let (cursor_sender, cursor_receiver) = mpsc::sync_channel(1);
        // Both commands share one line so tmux runs them back to back without
        // processing Pane output in between; the barrier follows the second.
        self.send_command_inner(
            &format!(
                "capture-pane -p -e -t {pane} ; display-message -p -t {pane} '#{{cursor_x}} #{{cursor_y}}'",
                pane = self.pane_id
            ),
            vec![
                (Some(screen_sender), None),
                (Some(cursor_sender), Some(Box::new(after_end))),
            ],
        )?;
        let receive = |receiver: mpsc::Receiver<Result<Vec<u8>, String>>| {
            receiver
                .recv_timeout(Duration::from_secs(15))
                .map_err(|error| format!("Could not capture Pane snapshot: {error}"))?
        };
        let screen = receive(screen_receiver)?;
        let cursor = receive(cursor_receiver)?;
        Ok(render_pane_snapshot(&screen, parse_pane_cursor(&cursor)))
    }

    pub fn close(&self) -> Result<(), String> {
        let mut closed = self
            .operation
            .lock()
            .map_err(|_| "tmux control client is unavailable".to_owned())?;
        *closed = true;
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
        self.send_command_inner(command, vec![(None, None)])
    }

    /// Sends one command line; `responses` holds one entry per tmux command in
    /// that line, in order, since tmux answers each with its own block.
    fn send_command_inner(
        &self,
        command: &str,
        responses: Vec<(
            Option<mpsc::SyncSender<Result<Vec<u8>, String>>>,
            Option<Box<dyn FnOnce() + Send>>,
        )>,
    ) -> Result<(), String> {
        let closed = self
            .operation
            .lock()
            .map_err(|_| "tmux control client is unavailable".to_owned())?;
        if *closed {
            return Err("tmux control client is closed".to_owned());
        }
        let mut input = self
            .input
            .lock()
            .map_err(|_| "tmux control client is unavailable".to_owned())?;
        let mut response_ids = Vec::with_capacity(responses.len());
        {
            let mut pending_responses = self
                .pending_responses
                .lock()
                .map_err(|_| "tmux control response queue is unavailable".to_owned())?;
            for (response_sender, after_end) in responses {
                let id = self.next_response_id.fetch_add(1, Ordering::Relaxed);
                response_ids.push(id);
                pending_responses.push_back(PendingControlCommand {
                    id,
                    response_sender,
                    after_end,
                });
            }
        }
        let write_result = input
            .write_all(command.as_bytes())
            .and_then(|_| input.write_all(b"\n"))
            .and_then(|_| input.flush());
        if let Err(error) = write_result {
            self.pending_responses
                .lock()
                .map_err(|_| "tmux control response queue is unavailable".to_owned())?
                .retain(|pending| !response_ids.contains(&pending.id));
            return Err(format!("Could not send command to Pane: {error}"));
        }
        Ok(())
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
    fn preflight_agent_run(
        &self,
        machine: &Machine,
        agent: AgentKind,
        run_id: i64,
        preferred_executable: Option<&Path>,
    ) -> MachineRunPreflight {
        prepare_machine_for_run(machine, Some((agent, run_id)), preferred_executable)
    }

    fn check_machine(&self, machine: &Machine) -> MachineReadiness {
        prepare_machine_for_run(machine, None, None).readiness
    }

    fn observe_machine(&self, machine: &Machine, state_run_ids: &[i64]) -> ObservedMachine {
        crate::terminal::observe_machine(machine, state_run_ids)
    }

    fn capture_pane_transcript(&self, machine: &Machine, pane_id: &str) -> Result<Vec<u8>, String> {
        crate::terminal::capture_pane_transcript(machine, pane_id)
    }

    fn list_panes(
        &self,
        machine: &Machine,
        session_name: &str,
    ) -> Result<Vec<PaneSummary>, String> {
        crate::terminal::list_panes(machine, session_name)
    }

    fn list_agent_panes(&self, machine: &Machine) -> Result<Vec<AgentPaneSummary>, String> {
        crate::terminal::list_agent_panes(machine)
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        gate_channel: &str,
        root: &Path,
        executable: &Path,
        prompt: &str,
        launch: AgentLaunchContext<'_>,
    ) -> Result<String, String> {
        launch_tmux_agent(
            machine,
            session_name,
            gate_channel,
            root,
            executable,
            prompt,
            launch,
        )
    }

    fn release_agent_launch(&self, machine: &Machine, gate_channel: &str) -> Result<(), String> {
        release_tmux_agent_launch(machine, gate_channel)
    }

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String> {
        kill_tmux_session(machine, session_name)
    }

    fn kill_pane(
        &self,
        machine: &Machine,
        _session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        kill_tmux_pane(machine, pane_id)
    }

    fn interrupt_pane(
        &self,
        machine: &Machine,
        _session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        interrupt_tmux_pane(machine, pane_id)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FakeMachineOutcome {
    Available,
    Unreachable,
    TmuxQueryFailed,
    TmuxUnavailable,
    AgentUnavailable,
    StateDirectoryUnwritable,
    HookProvisioningFailed,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FakeTerminalCommand {
    PreflightAgentRun {
        machine_id: i64,
        agent: AgentKind,
        run_id: i64,
    },
    CheckMachine {
        machine_id: i64,
    },
    ObserveMachine {
        machine_id: i64,
    },
    ReadAgentStateFiles {
        machine_id: i64,
        run_ids: Vec<i64>,
    },
    CapturePaneTranscript {
        machine_id: i64,
        pane_id: String,
    },
    ListPanes {
        machine_id: i64,
        session_name: String,
    },
    ListAgentPanes {
        machine_id: i64,
    },
    LaunchAgent {
        machine_id: i64,
        session_name: String,
        gate_channel: String,
        run_id: i64,
    },
    ReleaseAgentLaunch {
        machine_id: i64,
        gate_channel: String,
    },
    KillSession {
        machine_id: i64,
        session_name: String,
    },
    KillPane {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    SendPaneInput {
        machine_id: i64,
        pane_id: String,
        input: Vec<u8>,
    },
    InterruptPane {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
}

#[cfg(test)]
type FakeTranscriptCaptures =
    Arc<Mutex<std::collections::HashMap<(i64, String), Result<Vec<u8>, String>>>>;

#[cfg(test)]
pub(crate) struct FakeTerminalRuntime {
    outcomes: std::collections::HashMap<i64, FakeMachineOutcome>,
    commands: Arc<Mutex<Vec<FakeTerminalCommand>>>,
    hook_configs: Arc<Mutex<std::collections::HashMap<(i64, String), String>>>,
    observed_panes: Arc<Mutex<std::collections::HashMap<i64, Vec<ObservedPane>>>>,
    pane_state_records: Arc<Mutex<std::collections::HashMap<i64, Vec<ObservedPaneAgentState>>>>,
    state_file_records: Arc<Mutex<std::collections::HashMap<i64, Vec<AgentStateRecord>>>>,
    transcript_captures: FakeTranscriptCaptures,
    release_failures: Arc<Mutex<std::collections::HashMap<i64, String>>>,
    dirty_checkout_on_launch: Arc<Mutex<bool>>,
    checkout_remote_on_launch: Arc<Mutex<Option<String>>>,
    observation_gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
    send_gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
    release_gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
}

#[cfg(test)]
#[derive(Default)]
struct FakeObservationGateState {
    enabled: bool,
    started: bool,
    released: bool,
}

#[cfg(test)]
pub(crate) struct FakeObservationBlock {
    gate: Arc<(Mutex<FakeObservationGateState>, Condvar)>,
}

#[cfg(test)]
impl FakeObservationBlock {
    pub(crate) fn wait_until_started(&self, timeout: Duration) -> bool {
        let (state, changed) = &*self.gate;
        let state = state
            .lock()
            .expect("fake observation gate should remain available");
        if state.started {
            return true;
        }
        let (state, _) = changed
            .wait_timeout_while(state, timeout, |state| !state.started)
            .expect("fake observation gate should remain available");
        state.started
    }

    pub(crate) fn release(&self) {
        let (state, changed) = &*self.gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        state.enabled = false;
        state.released = true;
        changed.notify_all();
    }
}

#[cfg(test)]
impl Drop for FakeObservationBlock {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
impl FakeTerminalRuntime {
    pub(crate) fn new(outcomes: impl IntoIterator<Item = (i64, FakeMachineOutcome)>) -> Self {
        Self {
            outcomes: outcomes.into_iter().collect(),
            commands: Arc::new(Mutex::new(Vec::new())),
            hook_configs: Arc::new(Mutex::new(std::collections::HashMap::new())),
            observed_panes: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pane_state_records: Arc::new(Mutex::new(std::collections::HashMap::new())),
            state_file_records: Arc::new(Mutex::new(std::collections::HashMap::new())),
            transcript_captures: Arc::new(Mutex::new(std::collections::HashMap::new())),
            release_failures: Arc::new(Mutex::new(std::collections::HashMap::new())),
            dirty_checkout_on_launch: Arc::new(Mutex::new(false)),
            checkout_remote_on_launch: Arc::new(Mutex::new(None)),
            observation_gate: Arc::new((
                Mutex::new(FakeObservationGateState::default()),
                Condvar::new(),
            )),
            send_gate: Arc::new((
                Mutex::new(FakeObservationGateState::default()),
                Condvar::new(),
            )),
            release_gate: Arc::new((
                Mutex::new(FakeObservationGateState::default()),
                Condvar::new(),
            )),
        }
    }

    pub(crate) fn block_observations(&self) -> FakeObservationBlock {
        Self::block_gate(&self.observation_gate)
    }

    pub(crate) fn block_sends(&self) -> FakeObservationBlock {
        Self::block_gate(&self.send_gate)
    }

    pub(crate) fn block_launch_release(&self) -> FakeObservationBlock {
        Self::block_gate(&self.release_gate)
    }

    fn block_gate(gate: &Arc<(Mutex<FakeObservationGateState>, Condvar)>) -> FakeObservationBlock {
        let gate = Arc::clone(gate);
        let (state, _) = &*gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        state.enabled = true;
        state.started = false;
        state.released = false;
        drop(state);
        FakeObservationBlock { gate }
    }

    fn wait_for_gate(gate: &Arc<(Mutex<FakeObservationGateState>, Condvar)>) {
        let (state, changed) = &**gate;
        let mut state = state
            .lock()
            .expect("fake operation gate should remain available");
        if state.enabled {
            state.started = true;
            changed.notify_all();
            let _state = changed
                .wait_while(state, |state| !state.released)
                .expect("fake operation gate should remain available");
        }
    }

    pub(crate) fn set_observed_panes(&self, machine_id: i64, panes: Vec<ObservedPane>) {
        self.observed_panes
            .lock()
            .expect("fake observed pane list should remain available")
            .insert(machine_id, panes);
    }

    pub(crate) fn set_agent_state_records(&self, machine_id: i64, records: Vec<AgentStateRecord>) {
        self.state_file_records
            .lock()
            .expect("fake agent state records should remain available")
            .insert(machine_id, records);
    }

    pub(crate) fn set_pane_agent_state_records(
        &self,
        machine_id: i64,
        records: Vec<ObservedPaneAgentState>,
    ) {
        self.pane_state_records
            .lock()
            .expect("fake Pane agent state records should remain available")
            .insert(machine_id, records);
    }

    pub(crate) fn set_transcript_capture_failure(
        &self,
        machine_id: i64,
        pane_id: impl Into<String>,
        error: impl Into<String>,
    ) {
        self.transcript_captures
            .lock()
            .expect("fake transcript capture map should remain available")
            .insert((machine_id, pane_id.into()), Err(error.into()));
    }

    pub(crate) fn command_log(&self) -> Arc<Mutex<Vec<FakeTerminalCommand>>> {
        Arc::clone(&self.commands)
    }

    pub(crate) fn fail_launch_release(&self, machine_id: i64, error: impl Into<String>) {
        self.release_failures
            .lock()
            .expect("fake release failures should remain available")
            .insert(machine_id, error.into());
    }

    pub(crate) fn dirty_checkout_after_launch(&self) {
        *self
            .dirty_checkout_on_launch
            .lock()
            .expect("fake launch mutation should remain available") = true;
    }

    pub(crate) fn change_checkout_remote_after_launch(&self, remote_url: impl Into<String>) {
        *self
            .checkout_remote_on_launch
            .lock()
            .expect("fake launch mutation should remain available") = Some(remote_url.into());
    }

    pub(crate) fn set_agent_hook_config(
        &self,
        machine_id: i64,
        agent: AgentKind,
        contents: impl Into<String>,
    ) {
        self.hook_configs
            .lock()
            .expect("fake hook config should remain available")
            .insert((machine_id, agent.slug().into()), contents.into());
    }

    pub(crate) fn agent_hook_config(&self, machine_id: i64, agent: AgentKind) -> Option<String> {
        self.hook_configs
            .lock()
            .expect("fake hook config should remain available")
            .get(&(machine_id, agent.slug().into()))
            .cloned()
    }

    fn outcome(&self, machine: &Machine) -> FakeMachineOutcome {
        self.outcomes
            .get(&machine.id)
            .copied()
            .unwrap_or(FakeMachineOutcome::Unreachable)
    }

    fn record(&self, command: FakeTerminalCommand) {
        self.commands
            .lock()
            .expect("fake terminal command log should remain available")
            .push(command);
    }

    fn available_pane() -> PaneSummary {
        PaneSummary {
            pane_id: "%1".into(),
            pane_index: 0,
            pid: 1,
            columns: 80,
            rows: 24,
            title: "fake pane".into(),
            current_command: "fake-agent".into(),
            current_path: "/fake/worktree".into(),
        }
    }

    fn simulate_hook_provisioning(
        &self,
        machine: &Machine,
        readiness: &mut MachineReadiness,
        run: Option<(AgentKind, i64)>,
    ) {
        if readiness.reachable != Some(true)
            || (run.is_some() && readiness.state_directory_writable != Some(true))
            || self.outcome(machine) == FakeMachineOutcome::HookProvisioningFailed
        {
            return;
        }
        let script = Path::new("/fake/home").join(AGENT_STATE_HOOK_RELATIVE_PATH);
        for agent in [AgentKind::Claude, AgentKind::Codex] {
            let key = (machine.id, agent.slug().to_owned());
            let existing = self
                .hook_configs
                .lock()
                .expect("fake hook config should remain available")
                .get(&key)
                .cloned();
            let result = agent_state::merge_provider_hooks(
                existing.as_deref(),
                &script,
                agent,
                agent_state::agent_hook_events(agent),
            )
            .and_then(|merged| {
                let contents = String::from_utf8(merged).map_err(|error| error.to_string())?;
                self.hook_configs
                    .lock()
                    .expect("fake hook config should remain available")
                    .insert(key, contents);
                Ok(())
            });
            let status = match result {
                Ok(()) => hook_provisioning_success(),
                Err(error) => hook_provisioning_failure(error),
            };
            match agent {
                AgentKind::Claude => readiness.claude_hooks = status,
                AgentKind::Codex => readiness.codex_hooks = status,
            }
        }
        readiness.last_provisioning_error = [
            readiness.claude_hooks.error.as_deref(),
            readiness.codex_hooks.error.as_deref(),
        ]
        .into_iter()
        .flatten()
        .next()
        .map(str::to_owned);
        if readiness.error.is_none() {
            readiness.error = readiness.last_provisioning_error.clone();
        }
    }
}

#[cfg(test)]
impl TerminalRuntime for FakeTerminalRuntime {
    fn preflight_agent_run(
        &self,
        machine: &Machine,
        agent: AgentKind,
        run_id: i64,
        _preferred_executable: Option<&Path>,
    ) -> MachineRunPreflight {
        self.record(FakeTerminalCommand::PreflightAgentRun {
            machine_id: machine.id,
            agent,
            run_id,
        });
        let mut preflight =
            fake_machine_preflight(machine, self.outcome(machine), Some((agent, run_id)));
        self.simulate_hook_provisioning(machine, &mut preflight.readiness, Some((agent, run_id)));
        preflight
    }

    fn check_machine(&self, machine: &Machine) -> MachineReadiness {
        self.record(FakeTerminalCommand::CheckMachine {
            machine_id: machine.id,
        });
        let mut preflight = fake_machine_preflight(machine, self.outcome(machine), None);
        self.simulate_hook_provisioning(machine, &mut preflight.readiness, None);
        preflight.readiness
    }

    fn observe_machine(&self, machine: &Machine, state_run_ids: &[i64]) -> ObservedMachine {
        self.record(FakeTerminalCommand::ObserveMachine {
            machine_id: machine.id,
        });
        let (state, changed) = &*self.observation_gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        if state.enabled {
            state.started = true;
            changed.notify_all();
            state = changed
                .wait_while(state, |state| !state.released)
                .expect("fake observation gate should remain available");
        }
        drop(state);
        let panes = match self.outcome(machine) {
            FakeMachineOutcome::Available
            | FakeMachineOutcome::AgentUnavailable
            | FakeMachineOutcome::StateDirectoryUnwritable
            | FakeMachineOutcome::HookProvisioningFailed => Ok(self
                .observed_panes
                .lock()
                .expect("fake observed pane list should remain available")
                .get(&machine.id)
                .cloned()
                .unwrap_or_else(|| {
                    vec![ObservedPane {
                        session_name: format!("session-{}", machine.id),
                        pane_id: "%1".into(),
                    }]
                })),
            FakeMachineOutcome::Unreachable | FakeMachineOutcome::TmuxUnavailable => {
                Err(MachineObservationError {
                    kind: MachineObservationFailureKind::Unreachable,
                    message: format!("fake Machine {} is unreachable", machine.name),
                })
            }
            FakeMachineOutcome::TmuxQueryFailed => Err(MachineObservationError {
                kind: MachineObservationFailureKind::TmuxQueryFailed,
                message: format!("fake tmux query failed on Machine {}", machine.name),
            }),
        };
        let requested = state_run_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        self.record(FakeTerminalCommand::ReadAgentStateFiles {
            machine_id: machine.id,
            run_ids: state_run_ids.to_vec(),
        });
        let state_records = if matches!(
            &panes,
            Err(MachineObservationError {
                kind: MachineObservationFailureKind::Unreachable,
                ..
            })
        ) {
            Vec::new()
        } else {
            self.state_file_records
                .lock()
                .expect("fake agent state records should remain available")
                .get(&machine.id)
                .into_iter()
                .flatten()
                .filter(|record| {
                    record
                        .run_id
                        .parse::<i64>()
                        .is_ok_and(|run_id| requested.contains(&run_id))
                })
                .cloned()
                .collect()
        };
        let pane_state_records = self
            .pane_state_records
            .lock()
            .expect("fake Pane agent state records should remain available")
            .get(&machine.id)
            .into_iter()
            .flatten()
            .filter(|pane_state| {
                pane_state
                    .record
                    .run_id
                    .parse::<i64>()
                    .is_ok_and(|run_id| requested.contains(&run_id))
            })
            .cloned()
            .collect();
        ObservedMachine {
            panes,
            pane_state_records,
            state_file_records: state_records,
        }
    }

    fn capture_pane_transcript(&self, machine: &Machine, pane_id: &str) -> Result<Vec<u8>, String> {
        self.record(FakeTerminalCommand::CapturePaneTranscript {
            machine_id: machine.id,
            pane_id: pane_id.into(),
        });
        self.transcript_captures
            .lock()
            .expect("fake transcript capture map should remain available")
            .get(&(machine.id, pane_id.into()))
            .cloned()
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    fn list_panes(
        &self,
        machine: &Machine,
        session_name: &str,
    ) -> Result<Vec<PaneSummary>, String> {
        self.record(FakeTerminalCommand::ListPanes {
            machine_id: machine.id,
            session_name: session_name.into(),
        });
        match self.outcome(machine) {
            FakeMachineOutcome::Available
            | FakeMachineOutcome::AgentUnavailable
            | FakeMachineOutcome::StateDirectoryUnwritable
            | FakeMachineOutcome::HookProvisioningFailed => Ok(vec![Self::available_pane()]),
            FakeMachineOutcome::Unreachable | FakeMachineOutcome::TmuxUnavailable => {
                Err(format!("fake Machine {} is unreachable", machine.name))
            }
            FakeMachineOutcome::TmuxQueryFailed => Err(format!(
                "fake tmux query failed on Machine {}",
                machine.name
            )),
        }
    }

    fn list_agent_panes(&self, machine: &Machine) -> Result<Vec<AgentPaneSummary>, String> {
        self.record(FakeTerminalCommand::ListAgentPanes {
            machine_id: machine.id,
        });
        let (state, changed) = &*self.observation_gate;
        let mut state = state
            .lock()
            .expect("fake observation gate should remain available");
        if state.enabled {
            state.started = true;
            changed.notify_all();
            state = changed
                .wait_while(state, |state| !state.released)
                .expect("fake observation gate should remain available");
        }
        drop(state);
        match self.outcome(machine) {
            FakeMachineOutcome::Available
            | FakeMachineOutcome::AgentUnavailable
            | FakeMachineOutcome::StateDirectoryUnwritable
            | FakeMachineOutcome::HookProvisioningFailed => Ok(Vec::new()),
            FakeMachineOutcome::Unreachable | FakeMachineOutcome::TmuxUnavailable => {
                Err(format!("fake Machine {} is unreachable", machine.name))
            }
            FakeMachineOutcome::TmuxQueryFailed => Err(format!(
                "fake tmux query failed on Machine {}",
                machine.name
            )),
        }
    }

    fn send_pane_input(
        &self,
        machine: &Machine,
        pane_id: &str,
        input: &[u8],
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::SendPaneInput {
            machine_id: machine.id,
            pane_id: pane_id.into(),
            input: input.into(),
        });
        Self::wait_for_gate(&self.send_gate);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        gate_channel: &str,
        root: &Path,
        _executable: &Path,
        _prompt: &str,
        launch: AgentLaunchContext<'_>,
    ) -> Result<String, String> {
        self.record(FakeTerminalCommand::LaunchAgent {
            machine_id: machine.id,
            session_name: session_name.into(),
            gate_channel: gate_channel.into(),
            run_id: launch.run_id,
        });
        let dirty_after_launch = *self
            .dirty_checkout_on_launch
            .lock()
            .expect("fake launch mutation should remain available");
        if dirty_after_launch {
            std::fs::write(
                root.join(".fake-terminal-launch-dirt"),
                b"dirty after launch",
            )
            .map_err(|error| format!("Could not simulate post-launch checkout dirt: {error}"))?;
        }
        let remote_after_launch = self
            .checkout_remote_on_launch
            .lock()
            .expect("fake launch mutation should remain available")
            .clone();
        if let Some(remote_url) = remote_after_launch {
            let output = Command::new("git")
                .args(["-C", &root.to_string_lossy(), "remote", "set-url", "origin"])
                .arg(remote_url)
                .output()
                .map_err(|error| {
                    format!("Could not simulate post-launch remote change: {error}")
                })?;
            if !output.status.success() {
                return Err(format!(
                    "Could not simulate post-launch remote change: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        Ok(format!("%fake-{}", launch.run_id))
    }

    fn release_agent_launch(&self, machine: &Machine, gate_channel: &str) -> Result<(), String> {
        self.record(FakeTerminalCommand::ReleaseAgentLaunch {
            machine_id: machine.id,
            gate_channel: gate_channel.into(),
        });
        self.release_failures
            .lock()
            .expect("fake release failures should remain available")
            .get(&machine.id)
            .cloned()
            .map_or(Ok(()), Err)?;
        Self::wait_for_gate(&self.release_gate);
        Ok(())
    }

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String> {
        self.record(FakeTerminalCommand::KillSession {
            machine_id: machine.id,
            session_name: session_name.into(),
        });
        Ok(())
    }

    fn kill_pane(
        &self,
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::KillPane {
            machine_id: machine.id,
            session_name: session_name.into(),
            pane_id: pane_id.into(),
        });
        Ok(())
    }

    fn interrupt_pane(
        &self,
        machine: &Machine,
        session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        self.record(FakeTerminalCommand::InterruptPane {
            machine_id: machine.id,
            session_name: session_name.into(),
            pane_id: pane_id.into(),
        });
        Ok(())
    }
}

#[cfg(test)]
fn fake_machine_preflight(
    machine: &Machine,
    outcome: FakeMachineOutcome,
    run: Option<(AgentKind, i64)>,
) -> MachineRunPreflight {
    let mut readiness = MachineReadiness {
        reachable: Some(true),
        ..MachineReadiness::default()
    };
    if outcome == FakeMachineOutcome::Unreachable {
        readiness.reachable = Some(false);
        readiness.error = Some(format!("Could not reach Machine {}", machine.name));
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }
    if outcome == FakeMachineOutcome::TmuxUnavailable {
        readiness.tmux_available = Some(false);
        readiness.error = Some(format!(
            "Machine {} does not have tmux available",
            machine.name
        ));
        if run.is_some() {
            return MachineRunPreflight {
                readiness,
                ..MachineRunPreflight::default()
            };
        }
    } else {
        readiness.tmux_available = Some(true);
    }

    if let Some((agent, _)) = run {
        set_executable_readiness(
            &mut readiness,
            agent,
            Some(outcome != FakeMachineOutcome::AgentUnavailable),
        );
    }
    if let (FakeMachineOutcome::AgentUnavailable, Some((agent, _))) = (outcome, run) {
        readiness.error = Some(format!(
            "{} executable is unavailable on Machine {}",
            agent_display_name(agent),
            machine.name
        ));
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }

    if outcome == FakeMachineOutcome::StateDirectoryUnwritable {
        readiness.state_directory_writable = Some(false);
        readiness.error = Some(format!(
            "Agent state directory is not writable on Machine {}",
            machine.name
        ));
        if run.is_some() {
            return MachineRunPreflight {
                readiness,
                ..MachineRunPreflight::default()
            };
        }
    } else {
        readiness.state_directory_writable = Some(true);
    }

    if outcome == FakeMachineOutcome::HookProvisioningFailed {
        let error = "fake hook provisioning failed".to_owned();
        readiness.claude_hooks = hook_provisioning_failure(error.clone());
        readiness.codex_hooks = hook_provisioning_failure(error.clone());
        readiness.last_provisioning_error = Some(error.clone());
        readiness.error = Some(error);
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }
    readiness.claude_hooks = hook_provisioning_success();
    readiness.codex_hooks = hook_provisioning_success();
    let state_file =
        run.map(|(_, run_id)| state_file_for_machine_run(Path::new("/fake/home"), run_id));
    let executable = run.map(|(agent, _)| PathBuf::from(format!("/fake/bin/{}", agent.slug())));
    MachineRunPreflight {
        readiness,
        executable,
        state_file,
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
    let mut tmux_args = vec!["-f", "/dev/null", "-L", machine.socket_name.as_str()];
    tmux_args.extend(args.iter().map(String::as_str));
    let transport = terminal_transport(machine);
    let (program, arguments) = build_tmux_process_command(&transport, false, &tmux_args, "tmux")?;
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("Could not connect to Machine {}: {error}", machine.name))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            format!(
                "Machine {} tmux exited with {}",
                machine.name, output.status
            )
        } else {
            format!("Machine {}: {detail}", machine.name)
        })
    } else {
        Ok(output)
    }
}

pub(crate) fn kill_pane_with_timeout(
    machine: &Machine,
    pane_id: &str,
    timeout: Duration,
) -> Result<(), String> {
    let args = ["kill-pane".into(), "-t".into(), pane_id.into()];
    let mut tmux_args = vec!["-f", "/dev/null", "-L", machine.socket_name.as_str()];
    tmux_args.extend(args.iter().map(String::as_str));
    let (program, arguments) =
        build_tmux_process_command(&terminal_transport(machine), false, &tmux_args, "tmux")?;
    let mut child = Command::new(program)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not connect to Machine {}: {error}", machine.name))?;
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started_at.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Timed out after {} seconds stopping a Run pane on Machine {}",
                    timeout.as_secs(),
                    machine.name
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Could not stop a Run pane on Machine {}: {error}",
                    machine.name
                ));
            }
        }
    }
    let output = child.wait_with_output().map_err(|error| {
        format!(
            "Could not read the Run pane stop result from Machine {}: {error}",
            machine.name
        )
    })?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            format!(
                "Machine {} tmux exited with {}",
                machine.name, output.status
            )
        } else {
            format!("Machine {}: {detail}", machine.name)
        })
    }
}

pub fn terminal_transport(machine: &Machine) -> TerminalTransport {
    match &machine.transport {
        MachineTransport::Local => TerminalTransport::Local,
        MachineTransport::Ssh {
            host,
            user,
            port,
            identity_file,
            known_hosts_file,
            strict_host_key_checking,
        } => TerminalTransport::Ssh(SshTransport {
            host: host.clone(),
            user: user.clone(),
            port: *port,
            identity_file: identity_file.clone(),
            known_hosts_file: known_hosts_file.clone(),
            strict_host_key_checking: strict_host_key_checking.clone(),
            ssh_path: None,
        }),
    }
}

fn build_tmux_process_command(
    transport: &TerminalTransport,
    allocate_tty: bool,
    tmux_args: &[&str],
    tmux_path: &str,
) -> Result<(String, Vec<String>), String> {
    if tmux_path.trim().is_empty() {
        return Err("tmux executable path cannot be blank".to_owned());
    }
    if matches!(transport, TerminalTransport::Local) {
        return Ok((
            tmux_path.to_owned(),
            tmux_args
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
        ));
    }

    let TerminalTransport::Ssh(transport) = transport else {
        unreachable!("local transport returned above");
    };
    validate_ssh_transport(transport)?;
    let target = match &transport.user {
        Some(user) => format!("{user}@{}", transport.host),
        None => transport.host.clone(),
    };
    let remote_command = std::iter::once(tmux_path)
        .chain(tmux_args.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let mut arguments = vec![
        if allocate_tty { "-tt" } else { "-T" }.to_owned(),
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
    ];
    if let Some(port) = transport.port {
        arguments.extend(["-p".to_owned(), port.to_string()]);
    }
    if let Some(identity_file) = &transport.identity_file {
        arguments.extend(["-i".to_owned(), identity_file.clone()]);
    }
    if let Some(known_hosts_file) = &transport.known_hosts_file {
        arguments.extend([
            "-o".to_owned(),
            format!("UserKnownHostsFile={known_hosts_file}"),
        ]);
    }
    if let Some(strict_host_key_checking) = &transport.strict_host_key_checking {
        arguments.extend([
            "-o".to_owned(),
            format!("StrictHostKeyChecking={strict_host_key_checking}"),
        ]);
    }
    arguments.extend([target, remote_command]);
    Ok((
        transport.ssh_path.as_deref().unwrap_or("ssh").to_owned(),
        arguments,
    ))
}

pub fn probe_machine(machine: &Machine) -> Result<(), String> {
    match &machine.transport {
        MachineTransport::Local => Command::new("tmux")
            .arg("-V")
            .output()
            .map_err(|error| format!("Could not inspect Machine {}: {error}", machine.name))
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!("Machine {} could not run tmux", machine.name))
                }
            }),
        MachineTransport::Ssh { .. } => run_machine_shell(machine, "tmux -V").map(|_| ()),
    }
}

pub fn probe_local_runtime(executable: &Path) -> Result<(), String> {
    let output = Command::new(executable)
        .arg("-V")
        .output()
        .map_err(|error| format!("could not run tmux: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            format!("tmux exited with {}", output.status)
        } else {
            detail
        })
    }
}

pub fn find_agent_executable(machine: &Machine, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(format!("unsupported agent executable name: {name}"));
    }
    match machine.transport {
        MachineTransport::Local => env::var_os("PATH")
            .into_iter()
            .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join(name))
            .find(|candidate| is_executable(candidate))
            .and_then(|candidate| candidate.canonicalize().ok().or(Some(candidate)))
            .ok_or_else(|| format!("{name} is not installed on Machine {}", machine.name)),
        MachineTransport::Ssh { .. } => {
            let output = run_machine_shell(
                machine,
                &format!("command -v {} || true", shell_quote(name)),
            )?;
            let path = output.trim();
            if path.is_empty() {
                Err(format!(
                    "{name} is not installed on Machine {}",
                    machine.name
                ))
            } else if !Path::new(path).is_absolute() {
                Err(format!(
                    "Machine {} returned a non-absolute {name} executable path",
                    machine.name
                ))
            } else {
                Ok(PathBuf::from(path))
            }
        }
    }
}

pub(crate) fn run_machine_shell(machine: &Machine, command: &str) -> Result<String, String> {
    run_machine_shell_with_input(machine, command, &[])
}

fn run_machine_shell_with_input(
    machine: &Machine,
    command: &str,
    input: &[u8],
) -> Result<String, String> {
    let (program, arguments) = build_machine_shell_command(machine, command)?;
    let output = run_shell_with_input(&program, &arguments, input).map_err(|error| {
        let kind = if matches!(machine.transport, MachineTransport::Ssh { .. }) {
            "connect to"
        } else {
            "inspect"
        };
        format!("Could not {kind} Machine {}: {error}", machine.name)
    })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if detail.is_empty() {
            format!("command exited with {}", output.status)
        } else {
            detail
        })
    }
}

fn build_machine_shell_command(
    machine: &Machine,
    command: &str,
) -> Result<(String, Vec<String>), String> {
    match &machine.transport {
        MachineTransport::Local => Ok(("sh".into(), vec!["-lc".into(), command.into()])),
        MachineTransport::Ssh { .. } => {
            let transport = terminal_transport(machine);
            let TerminalTransport::Ssh(transport) = transport else {
                unreachable!("SSH transport should remain SSH");
            };
            validate_ssh_transport(&transport)?;
            let target = match &transport.user {
                Some(user) => format!("{user}@{}", transport.host),
                None => transport.host.clone(),
            };
            let mut arguments = vec!["-T".to_owned(), "-o".to_owned(), "BatchMode=yes".to_owned()];
            if let Some(port) = transport.port {
                arguments.extend(["-p".to_owned(), port.to_string()]);
            }
            if let Some(identity_file) = &transport.identity_file {
                arguments.extend(["-i".to_owned(), identity_file.clone()]);
            }
            if let Some(known_hosts_file) = &transport.known_hosts_file {
                arguments.extend([
                    "-o".to_owned(),
                    format!("UserKnownHostsFile={known_hosts_file}"),
                ]);
            }
            if let Some(strict_host_key_checking) = &transport.strict_host_key_checking {
                arguments.extend([
                    "-o".to_owned(),
                    format!("StrictHostKeyChecking={strict_host_key_checking}"),
                ]);
            }
            arguments.extend([target, command.to_owned()]);
            Ok((
                transport.ssh_path.as_deref().unwrap_or("ssh").to_owned(),
                arguments,
            ))
        }
    }
}

fn run_shell_with_input(
    program: &str,
    arguments: &[impl AsRef<std::ffi::OsStr>],
    input: &[u8],
) -> std::io::Result<std::process::Output> {
    if input.is_empty() {
        return Command::new(program).args(arguments).output();
    }
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped shell stdin should be available")
        .write_all(input)?;
    child.wait_with_output()
}

#[derive(Clone, Copy)]
enum RemoteHookFile {
    Script,
    ClaudeSettings,
    CodexHooks,
}

fn remote_hook_file_path(file: RemoteHookFile) -> &'static str {
    match file {
        RemoteHookFile::Script => crate::agent_state::AGENT_STATE_HOOK_RELATIVE_PATH,
        RemoteHookFile::ClaudeSettings => ".claude/settings.json",
        RemoteHookFile::CodexHooks => ".codex/hooks.json",
    }
}

fn build_remote_hook_read_command(agent: AgentKind) -> String {
    let file = match agent {
        AgentKind::Claude => RemoteHookFile::ClaudeSettings,
        AgentKind::Codex => RemoteHookFile::CodexHooks,
    };
    let path = remote_hook_file_path(file);
    format!(
        "target=\"$HOME/{path}\"; if [ -L \"$target\" ] || {{ [ -e \"$target\" ] && [ ! -f \"$target\" ]; }}; then printf '%s\\n' 'agent hook target is not a regular file' >&2; exit 1; fi; if [ -f \"$target\" ]; then printf '\\001'; cat \"$target\"; else printf '\\002'; fi"
    )
}

fn build_remote_hook_write_command(agent: AgentKind) -> String {
    let file = match agent {
        AgentKind::Claude => RemoteHookFile::ClaudeSettings,
        AgentKind::Codex => RemoteHookFile::CodexHooks,
    };
    build_remote_atomic_file_write_command(file, 0o600, false)
}

fn build_remote_hook_script_install_command() -> String {
    build_remote_atomic_file_write_command(RemoteHookFile::Script, 0o700, true)
}

fn build_remote_atomic_file_write_command(
    file: RemoteHookFile,
    mode: u32,
    repair_existing_executable_mode: bool,
) -> String {
    let path = remote_hook_file_path(file);
    let existing_mode_repair = if repair_existing_executable_mode {
        "if [ ! -x \"$target\" ]; then chmod 700 \"$target\"; fi; "
    } else {
        ""
    };
    format!("set -eu; umask 077; target=\"$HOME/{path}\"; parent=${{target%/*}}; mkdir -p \"$parent\"; if [ -L \"$target\" ] || {{ [ -e \"$target\" ] && [ ! -f \"$target\" ]; }}; then printf '%s\\n' 'agent hook target is not a regular file' >&2; exit 1; fi; temporary=\"$target.tmp.$$\"; trap 'rm -f \"$temporary\"' EXIT HUP INT TERM; (set -C; : > \"$temporary\"); cat > \"$temporary\"; chmod {mode:o} \"$temporary\"; if [ -f \"$target\" ] && cmp -s \"$target\" \"$temporary\"; then {existing_mode_repair}rm -f \"$temporary\"; else mv -f \"$temporary\" \"$target\"; fi; if [ -L \"$target\" ] || [ ! -f \"$target\" ]; then printf '%s\\n' 'agent hook target was not written as a regular file' >&2; exit 1; fi; trap - EXIT HUP INT TERM")
}

fn build_remote_state_directory_probe_command() -> String {
    "set -eu; state_dir=\"$HOME/.local/state/ai-mission-manager/runs\"; umask 077; mkdir -p \"$state_dir\"; temporary=\"$state_dir/.preflight.$$\"; (set -C; : > \"$temporary\"); rm -f \"$temporary\"".into()
}

fn read_remote_hook_file(machine: &Machine, agent: AgentKind) -> Result<Option<String>, String> {
    let output = run_machine_shell(machine, &build_remote_hook_read_command(agent))?;
    let Some(marker) = output.get(..1) else {
        return Err(format!(
            "Machine {} returned an invalid provider hooks response",
            machine.name
        ));
    };
    let contents = &output[1..];
    match marker {
        "\u{1}" => Ok(Some(contents.to_owned())),
        "\u{2}" if contents.is_empty() => Ok(None),
        _ => Err(format!(
            "Machine {} returned an invalid provider hooks response",
            machine.name
        )),
    }
}

fn write_remote_hook_file(
    machine: &Machine,
    agent: AgentKind,
    contents: &[u8],
) -> Result<(), String> {
    run_machine_shell_with_input(machine, &build_remote_hook_write_command(agent), contents)
        .map(|_| ())
}

fn install_remote_hook_script(machine: &Machine) -> Result<(), String> {
    run_machine_shell_with_input(
        machine,
        &build_remote_hook_script_install_command(),
        crate::agent_state::AGENT_STATE_HOOK_SCRIPT.as_bytes(),
    )
    .map(|_| ())
}

fn prepare_machine_for_run(
    machine: &Machine,
    run: Option<(AgentKind, i64)>,
    preferred_executable: Option<&Path>,
) -> MachineRunPreflight {
    let mut readiness = MachineReadiness::default();
    let home = match &machine.transport {
        MachineTransport::Local => match env::var_os("HOME") {
            Some(home) => PathBuf::from(home),
            None => {
                readiness.reachable = Some(true);
                readiness.error = Some("HOME is not set on the local Machine".into());
                return MachineRunPreflight {
                    readiness,
                    ..MachineRunPreflight::default()
                };
            }
        },
        MachineTransport::Ssh { .. } => match run_machine_shell(machine, "printf '%s' \"$HOME\"") {
            Ok(home) if Path::new(home.trim()).is_absolute() => PathBuf::from(home.trim()),
            Ok(_) => {
                readiness.reachable = Some(true);
                readiness.error = Some(format!(
                    "Machine {} did not report an absolute home directory",
                    machine.name
                ));
                return MachineRunPreflight {
                    readiness,
                    ..MachineRunPreflight::default()
                };
            }
            Err(error) => {
                readiness.reachable = Some(false);
                readiness.error =
                    Some(format!("Could not reach Machine {}: {error}", machine.name));
                return MachineRunPreflight {
                    readiness,
                    ..MachineRunPreflight::default()
                };
            }
        },
    };
    readiness.reachable = Some(true);

    if let Err(error) = probe_machine(machine) {
        readiness.tmux_available = Some(false);
        readiness.error = Some(format!(
            "Machine {} does not have a working tmux runtime: {error}",
            machine.name
        ));
        if run.is_some() {
            return MachineRunPreflight {
                readiness,
                ..MachineRunPreflight::default()
            };
        }
    } else {
        readiness.tmux_available = Some(true);
    }

    let mut executable = None;
    if let Some((agent, _)) = run {
        match resolve_run_executable(machine, agent, preferred_executable) {
            Ok(path) => {
                set_executable_readiness(&mut readiness, agent, Some(true));
                executable = Some(path);
            }
            Err(error) => {
                set_executable_readiness(&mut readiness, agent, Some(false));
                readiness.error = Some(error);
                return MachineRunPreflight {
                    readiness,
                    ..MachineRunPreflight::default()
                };
            }
        }
    }

    let state_directory = match &machine.transport {
        MachineTransport::Local => agent_state::ensure_state_runs_directory(&home),
        MachineTransport::Ssh { .. } => {
            run_machine_shell(machine, &build_remote_state_directory_probe_command())
                .map(|_| home.join(AGENT_STATE_RUNS_RELATIVE_PATH))
        }
    };
    match state_directory {
        Ok(_directory) => readiness.state_directory_writable = Some(true),
        Err(error) => {
            readiness.state_directory_writable = Some(false);
            if readiness.error.is_none() {
                readiness.error = Some(format!(
                    "Agent state directory is not writable on Machine {}: {error}",
                    machine.name
                ));
            }
            if run.is_some() {
                return MachineRunPreflight {
                    readiness,
                    ..MachineRunPreflight::default()
                };
            }
        }
    }

    let (claude_hooks, codex_hooks) = match &machine.transport {
        MachineTransport::Local => provision_local_agent_hooks(&home),
        MachineTransport::Ssh { .. } => provision_remote_agent_hooks(machine, &home),
    };
    readiness.claude_hooks = claude_hooks;
    readiness.codex_hooks = codex_hooks;
    readiness.last_provisioning_error = [
        readiness.claude_hooks.error.as_deref(),
        readiness.codex_hooks.error.as_deref(),
    ]
    .into_iter()
    .flatten()
    .next()
    .map(str::to_owned);
    if readiness.error.is_none() {
        readiness.error = readiness.last_provisioning_error.clone();
    }

    if readiness.error.is_some() {
        return MachineRunPreflight {
            readiness,
            ..MachineRunPreflight::default()
        };
    }

    let state_file = run.map(|(_, run_id)| state_file_for_machine_run(&home, run_id));
    MachineRunPreflight {
        readiness,
        executable,
        state_file,
    }
}

fn resolve_run_executable(
    machine: &Machine,
    agent: AgentKind,
    preferred_executable: Option<&Path>,
) -> Result<PathBuf, String> {
    let name = agent.slug();
    let path = if matches!(machine.transport, MachineTransport::Local) {
        match preferred_executable {
            Some(path) if is_executable(path) => Ok(path.to_owned()),
            Some(path) => Err(format!(
                "{} executable {} is not available on Machine {}",
                agent_display_name(agent),
                path.display(),
                machine.name
            )),
            None => find_agent_executable(machine, name),
        }
    } else {
        find_agent_executable(machine, name)
    };
    path.map_err(|error| {
        format!(
            "{} executable is unavailable on Machine {}: {error}",
            agent_display_name(agent),
            machine.name
        )
    })
}

fn provision_local_agent_hooks(home: &Path) -> (AgentHookReadiness, AgentHookReadiness) {
    let script = agent_state::install_agent_state_hook(home);
    let mut claude = match &script {
        Ok(_) => provision_local_provider_hooks(home, AgentKind::Claude),
        Err(error) => hook_provisioning_failure(error.clone()),
    };
    let mut codex = match &script {
        Ok(_) => provision_local_provider_hooks(home, AgentKind::Codex),
        Err(error) => hook_provisioning_failure(error.clone()),
    };
    if script.is_err() {
        claude.current = Some(false);
        codex.current = Some(false);
    }
    (claude, codex)
}

fn provision_local_provider_hooks(home: &Path, agent: AgentKind) -> AgentHookReadiness {
    match agent_state::provision_agent_hooks_for(home, agent) {
        Ok(()) => hook_provisioning_success(),
        Err(error) => hook_provisioning_failure(error),
    }
}

fn provision_remote_agent_hooks(
    machine: &Machine,
    home: &Path,
) -> (AgentHookReadiness, AgentHookReadiness) {
    if let Err(error) = install_remote_hook_script(machine) {
        let message = format!("Could not provision the agent state hook script: {error}");
        return (
            hook_provisioning_failure(message.clone()),
            hook_provisioning_failure(message),
        );
    }
    let script = home.join(AGENT_STATE_HOOK_RELATIVE_PATH);
    (
        provision_remote_provider_hooks(machine, &script, AgentKind::Claude),
        provision_remote_provider_hooks(machine, &script, AgentKind::Codex),
    )
}

fn provision_remote_provider_hooks(
    machine: &Machine,
    script: &Path,
    agent: AgentKind,
) -> AgentHookReadiness {
    let result = (|| {
        let existing = read_remote_hook_file(machine, agent)?;
        let merged = agent_state::merge_provider_hooks(
            existing.as_deref(),
            script,
            agent,
            agent_state::agent_hook_events(agent),
        )
        .map_err(|error| {
            format!(
                "Could not merge {} hooks: {error}",
                agent_display_name(agent)
            )
        })?;
        if existing
            .as_deref()
            .is_none_or(|current| current.as_bytes() != merged)
        {
            write_remote_hook_file(machine, agent, &merged).map_err(|error| {
                format!(
                    "Could not write {} hooks atomically: {error}",
                    agent_display_name(agent)
                )
            })?;
        }
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => hook_provisioning_success(),
        Err(error) => hook_provisioning_failure(error),
    }
}

fn hook_provisioning_success() -> AgentHookReadiness {
    AgentHookReadiness {
        provisioned: Some(true),
        current: Some(true),
        error: None,
    }
}

fn hook_provisioning_failure(error: String) -> AgentHookReadiness {
    AgentHookReadiness {
        provisioned: Some(false),
        current: Some(false),
        error: Some(error),
    }
}

fn set_executable_readiness(
    readiness: &mut MachineReadiness,
    agent: AgentKind,
    result: Option<bool>,
) {
    match agent {
        AgentKind::Claude => readiness.claude_executable_resolved = result,
        AgentKind::Codex => readiness.codex_executable_resolved = result,
    }
}

fn agent_display_name(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "Claude Code",
        AgentKind::Codex => "Codex",
    }
}

fn state_file_for_machine_run(home: &Path, run_id: i64) -> PathBuf {
    agent_state::state_file_path(&home.join(AGENT_STATE_RUNS_RELATIVE_PATH), run_id)
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
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

pub fn observe_machine(machine: &Machine, state_run_ids: &[i64]) -> ObservedMachine {
    let panes = observe_machine_panes(machine);
    let pane_state_records = panes
        .as_ref()
        .map(|panes| {
            panes
                .iter()
                .filter_map(|pane| {
                    pane_agent_state(&pane.agent_state_json).map(|record| ObservedPaneAgentState {
                        session_name: pane.session_name.clone(),
                        pane_id: pane.pane_id.clone(),
                        record,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let state_file_records = read_agent_state_files(machine, state_run_ids);
    ObservedMachine {
        panes: panes.map(|panes| {
            panes
                .into_iter()
                .map(|pane| ObservedPane {
                    session_name: pane.session_name,
                    pane_id: pane.pane_id,
                })
                .collect()
        }),
        pane_state_records,
        state_file_records,
    }
}

struct ObservedPaneWithState {
    session_name: String,
    pane_id: String,
    agent_state_json: String,
}

fn observe_machine_panes(
    machine: &Machine,
) -> Result<Vec<ObservedPaneWithState>, MachineObservationError> {
    let mut tmux_args = vec!["-f", "/dev/null", "-L", machine.socket_name.as_str()];
    let pane_format = machine_pane_query_format();
    tmux_args.extend(["list-panes", "-a", "-F", pane_format.as_str()]);
    let (program, arguments) =
        build_tmux_process_command(&terminal_transport(machine), false, &tmux_args, "tmux")
            .map_err(|message| MachineObservationError {
                kind: MachineObservationFailureKind::TmuxQueryFailed,
                message,
            })?;
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| MachineObservationError {
            kind: if matches!(machine.transport, MachineTransport::Ssh { .. }) {
                MachineObservationFailureKind::Unreachable
            } else {
                MachineObservationFailureKind::TmuxQueryFailed
            },
            message: format!("Could not observe Machine {}: {error}", machine.name),
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if detail.is_empty() {
            format!(
                "Machine {} tmux exited with {}",
                machine.name, output.status
            )
        } else {
            format!("Machine {}: {detail}", machine.name)
        };
        let unreachable = matches!(machine.transport, MachineTransport::Ssh { .. })
            && is_ssh_connection_failure(&detail);
        return Err(MachineObservationError {
            kind: if unreachable {
                MachineObservationFailureKind::Unreachable
            } else {
                MachineObservationFailureKind::TmuxQueryFailed
            },
            message,
        });
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            let Some((session_name, remaining)) = line.split_once('\t') else {
                return Err(MachineObservationError {
                    kind: MachineObservationFailureKind::TmuxQueryFailed,
                    message: format!(
                        "Could not parse Machine {} tmux Pane observation",
                        machine.name
                    ),
                });
            };
            let Some((pane_id, agent_state_json)) = remaining.split_once('\t') else {
                return Err(MachineObservationError {
                    kind: MachineObservationFailureKind::TmuxQueryFailed,
                    message: format!(
                        "Could not parse Machine {} tmux Pane observation",
                        machine.name
                    ),
                });
            };
            if session_name.is_empty() || pane_id.is_empty() {
                return Err(MachineObservationError {
                    kind: MachineObservationFailureKind::TmuxQueryFailed,
                    message: format!(
                        "Could not parse Machine {} tmux Pane observation",
                        machine.name
                    ),
                });
            }
            Ok(ObservedPaneWithState {
                session_name: session_name.to_owned(),
                pane_id: pane_id.to_owned(),
                agent_state_json: agent_state_json.to_owned(),
            })
        })
        .collect()
}

fn machine_pane_query_format() -> String {
    format!("#{{session_name}}\t#{{pane_id}}\t#{{{AGENT_STATE_OPTION}}}")
}

fn pane_agent_state(value: &str) -> Option<AgentStateRecord> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    serde_json::from_str(value).ok()
}

fn read_agent_state_files(machine: &Machine, run_ids: &[i64]) -> Vec<AgentStateRecord> {
    let Some(command) = build_agent_state_files_read_command(run_ids) else {
        return Vec::new();
    };
    match run_machine_shell(machine, &command) {
        Ok(contents) => parse_agent_state_files_read(&contents, run_ids),
        Err(error) => {
            eprintln!(
                "Could not read agent state files on Machine {}: {error}",
                machine.name
            );
            Vec::new()
        }
    }
}

fn build_agent_state_files_read_command(run_ids: &[i64]) -> Option<String> {
    let mut run_ids = run_ids
        .iter()
        .copied()
        .filter(|run_id| *run_id > 0)
        .collect::<Vec<_>>();
    run_ids.sort_unstable();
    run_ids.dedup();
    if run_ids.is_empty() {
        return None;
    }
    let run_ids = run_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!(
        "set -eu; state_dir=\"$HOME/{AGENT_STATE_RUNS_RELATIVE_PATH}\"; for run_id in {run_ids}; do state_file=\"$state_dir/run-$run_id.json\"; if [ -f \"$state_file\" ] && [ ! -L \"$state_file\" ] && [ -r \"$state_file\" ]; then printf '%s\\000' \"$run_id\"; if cat \"$state_file\"; then :; fi; printf '\\000'; fi; done"
    ))
}

fn parse_agent_state_files_read(
    contents: &str,
    requested_run_ids: &[i64],
) -> Vec<AgentStateRecord> {
    let requested_run_ids = requested_run_ids
        .iter()
        .copied()
        .filter(|run_id| *run_id > 0)
        .collect::<std::collections::HashSet<_>>();
    let Some(last_complete_record) = contents.rfind('\0') else {
        return Vec::new();
    };
    let mut fields = contents[..last_complete_record].split('\0');
    let mut records = Vec::new();
    while let Some(path_run_id) = fields.next() {
        let Some(record_json) = fields.next() else {
            break;
        };
        let Some(path_run_id) = path_run_id.parse::<i64>().ok() else {
            continue;
        };
        if !requested_run_ids.contains(&path_run_id) {
            continue;
        }
        let Ok(record) = serde_json::from_str::<AgentStateRecord>(record_json) else {
            continue;
        };
        if record.run_id.parse::<i64>().ok() == Some(path_run_id) {
            records.push(record);
        }
    }
    records
}

fn is_ssh_connection_failure(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    [
        "could not resolve hostname",
        "connection refused",
        "connection timed out",
        "operation timed out",
        "no route to host",
        "network is unreachable",
        "connection reset by peer",
        "connection closed by",
        "ssh: connect to host",
        "permission denied (publickey",
        "host key verification failed",
    ]
    .iter()
    .any(|pattern| detail.contains(pattern))
}

pub fn list_agent_panes(machine: &Machine) -> Result<Vec<AgentPaneSummary>, String> {
    let output = run_tmux(
        machine,
        &[
            "list-panes".into(),
            "-a".into(),
            "-F".into(),
            "#{session_name}\t#{pane_id}\t#{pane_current_command}\t#{pane_title}\t#{pane_current_path}".into(),
        ],
    )?;
    output
        .lines()
        .filter_map(|line| parse_agent_pane_summary(line).transpose())
        .collect::<Result<Vec<_>, _>>()
}

pub fn capture_pane_transcript(machine: &Machine, pane_id: &str) -> Result<Vec<u8>, String> {
    validate_pane_id(pane_id)?;
    let output = run_tmux_output(
        machine,
        &[
            "capture-pane".into(),
            "-p".into(),
            // Join lines the terminal wrapped, so long event lines and URLs
            // stay whole.
            "-J".into(),
            "-S".into(),
            "-".into(),
            "-t".into(),
            pane_id.into(),
        ],
    )?;
    Ok(output.stdout)
}

pub fn send_input_to_pane(machine: &Machine, pane_id: &str, input: &[u8]) -> Result<(), String> {
    validate_pane_id(pane_id)?;
    if input.is_empty() {
        return Ok(());
    }
    let (text, should_submit) = match input.strip_suffix(b"\n") {
        Some(text) => (text, true),
        None => (input, false),
    };
    if !text.is_empty() {
        let text = std::str::from_utf8(text)
            .map_err(|error| format!("Terminal input is not valid UTF-8: {error}"))?;
        let buffer_name = format!(
            "ai-mission-manager-input-{}",
            pane_id.trim_start_matches('%')
        );
        run_tmux(
            machine,
            &[
                "set-buffer".into(),
                "-b".into(),
                buffer_name.clone(),
                text.into(),
            ],
        )?;
        run_tmux(
            machine,
            &[
                "paste-buffer".into(),
                "-r".into(),
                "-p".into(),
                "-d".into(),
                "-b".into(),
                buffer_name,
                "-t".into(),
                pane_id.into(),
            ],
        )?;
    }
    if should_submit {
        run_tmux(
            machine,
            &[
                "send-keys".into(),
                "-t".into(),
                pane_id.into(),
                "C-m".into(),
            ],
        )?;
    }
    Ok(())
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

fn parse_agent_pane_summary(line: &str) -> Result<Option<AgentPaneSummary>, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(format!(
            "tmux returned an invalid agent Pane description: {line}"
        ));
    }
    let Some(agent) =
        agent_kind_from_command(fields[2]).or_else(|| agent_kind_from_command(fields[3]))
    else {
        return Ok(None);
    };
    Ok(Some(AgentPaneSummary {
        agent,
        session_name: fields[0].to_owned(),
        pane_id: fields[1].to_owned(),
        current_path: fields[4].to_owned(),
    }))
}

fn agent_kind_from_command(command: &str) -> Option<AgentKind> {
    let command = command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match command.as_str() {
        "claude" | "claude-code" => Some(AgentKind::Claude),
        "codex" => Some(AgentKind::Codex),
        _ => None,
    }
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

/// `capture-pane -p` separates rows with bare LF, which a terminal emulator
/// treats as "down one row, same column". Replaying it verbatim shifts every
/// row right by the previous row's width, so rows are rejoined with CRLF and
/// the cursor is put back where the Pane has it.
fn render_pane_snapshot(screen: &[u8], cursor: Option<(u16, u16)>) -> Vec<u8> {
    let screen = screen.strip_suffix(b"\n").unwrap_or(screen);
    let mut rows = screen.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    while rows.len() > 1 && rows.last().is_some_and(|row| row.is_empty()) {
        rows.pop();
    }
    let mut rendered = rows.join(&b"\r\n"[..]);
    rendered.extend_from_slice(b"\x1b[0m");
    if let Some((column, row)) = cursor {
        rendered.extend_from_slice(format!("\x1b[{};{}H", row + 1, column + 1).as_bytes());
    }
    rendered
}

fn parse_pane_cursor(response: &[u8]) -> Option<(u16, u16)> {
    let text = std::str::from_utf8(response).ok()?;
    let (column, row) = text.trim().split_once(' ')?;
    Some((column.parse().ok()?, row.parse().ok()?))
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
    gate_channel: &str,
    root: &Path,
    executable: &Path,
    prompt: &str,
    launch: AgentLaunchContext<'_>,
) -> Result<String, String> {
    validate_tmux_target(session_name)?;
    let command = build_gated_agent_command(
        &machine.socket_name,
        gate_channel,
        executable,
        prompt,
        &launch,
    )?;
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

fn build_gated_agent_command(
    socket_name: &str,
    gate_channel: &str,
    executable: &Path,
    prompt: &str,
    launch: &AgentLaunchContext<'_>,
) -> Result<String, String> {
    validate_tmux_target(socket_name)?;
    validate_tmux_target(gate_channel)?;
    let mut command = format!(
        "{} && {} && export AI_MISSION_MANAGER_RUN_ID={}",
        build_tmux_pane_title_command("tmux", socket_name, launch.agent.slug())?,
        build_tmux_wait_for_command("tmux", socket_name, gate_channel, false)?,
        shell_quote(&launch.run_id.to_string()),
    );
    for assignment in [
        format!(
            "export AI_MISSION_MANAGER_STATE_FILE={}",
            shell_quote(&launch.state_file.to_string_lossy())
        ),
        "export AI_MISSION_MANAGER_TMUX_PATH=tmux".to_owned(),
        format!(
            "export AI_MISSION_MANAGER_TMUX_SOCKET={}",
            shell_quote(socket_name)
        ),
        "export AI_MISSION_MANAGER_PANE_ID=\"$TMUX_PANE\"".to_owned(),
    ] {
        command.push_str(" && ");
        command.push_str(&assignment);
    }
    command.push_str(" && exec ");
    command.push_str(&shell_quote(&executable.to_string_lossy()));
    for argument in agent_cli_arguments(launch.agent, launch.model, launch.effort)? {
        command.push(' ');
        command.push_str(&shell_quote(&argument));
    }
    command.push(' ');
    command.push_str(&shell_quote(prompt));
    Ok(command)
}

fn build_tmux_pane_title_command(
    tmux_path: &str,
    socket_name: &str,
    title: &str,
) -> Result<String, String> {
    if tmux_path.trim().is_empty() {
        return Err("tmux executable path cannot be blank".to_owned());
    }
    validate_tmux_target(socket_name)?;
    if title.trim().is_empty() {
        return Err("tmux Pane title cannot be blank".to_owned());
    }
    let arguments = [
        tmux_path,
        "-f",
        "/dev/null",
        "-L",
        socket_name,
        "select-pane",
        "-T",
        title,
    ];
    let mut command = arguments
        .into_iter()
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    command.push_str(" -t \"$TMUX_PANE\"");
    Ok(command)
}

fn build_tmux_wait_for_command(
    tmux_path: &str,
    socket_name: &str,
    gate_channel: &str,
    release: bool,
) -> Result<String, String> {
    if tmux_path.trim().is_empty() {
        return Err("tmux executable path cannot be blank".to_owned());
    }
    validate_tmux_target(socket_name)?;
    validate_tmux_target(gate_channel)?;
    let mut arguments = vec![tmux_path, "-f", "/dev/null", "-L", socket_name, "wait-for"];
    if release {
        arguments.push("-S");
    }
    arguments.push(gate_channel);
    Ok(arguments
        .into_iter()
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" "))
}

fn release_tmux_agent_launch(machine: &Machine, gate_channel: &str) -> Result<(), String> {
    let args = ["wait-for".into(), "-S".into(), gate_channel.into()];
    run_tmux(machine, &args).map(|_| ())
}

pub fn agent_cli_arguments(
    agent: AgentKind,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut arguments = Vec::new();
    if let Some(model) = model {
        if model.trim().is_empty() {
            return Err("agent model identifier cannot be blank".into());
        }
        let model = match agent {
            AgentKind::Claude => match model {
                "claude-opus-4-1" => "claude-opus-5",
                provider_model => provider_model,
            },
            AgentKind::Codex => match model {
                "codex-sol" | "codex-terra" => "gpt-6-sol",
                "codex-luna" => "gpt-6-luna",
                provider_model => provider_model,
            },
        };
        arguments.extend(["--model".into(), model.into()]);
    }
    if let Some(effort) = effort {
        if effort.trim().is_empty() {
            return Err("agent effort identifier cannot be blank".into());
        }
        match agent {
            AgentKind::Claude => arguments.extend(["--effort".into(), effort.into()]),
            AgentKind::Codex => {
                arguments.extend(["-c".into(), format!("model_reasoning_effort={effort}")])
            }
        }
    }
    Ok(arguments)
}

fn kill_tmux_session(machine: &Machine, session_name: &str) -> Result<(), String> {
    let args = vec!["kill-session".into(), "-t".into(), session_name.into()];
    run_tmux(machine, &args).map(|_| ())
}

fn kill_tmux_pane(machine: &Machine, pane_id: &str) -> Result<(), String> {
    let args = vec!["kill-pane".into(), "-t".into(), pane_id.into()];
    run_tmux(machine, &args).map(|_| ())
}

fn interrupt_tmux_pane(machine: &Machine, pane_id: &str) -> Result<(), String> {
    validate_pane_id(pane_id)?;
    let args = vec![
        "send-keys".into(),
        "-t".into(),
        pane_id.into(),
        "C-c".into(),
    ];
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
    use std::{path::Path, sync::mpsc};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn grill_launch_uses_provider_identifiers_for_model_and_effort() {
        assert_eq!(
            agent_cli_arguments(AgentKind::Claude, Some("claude-sonnet-4-5"), Some("high"))
                .expect("Claude arguments should be valid"),
            vec!["--model", "claude-sonnet-4-5", "--effort", "high"]
        );
        assert_eq!(
            agent_cli_arguments(AgentKind::Codex, Some("codex-luna"), Some("xhigh"))
                .expect("Codex arguments should be valid"),
            vec![
                "--model",
                "gpt-6-luna",
                "-c",
                "model_reasoning_effort=xhigh"
            ]
        );
    }

    #[test]
    fn implement_launch_uses_provider_identifiers_for_model_and_effort() {
        assert_eq!(
            agent_cli_arguments(AgentKind::Codex, Some("gpt-6-luna"), Some("xhigh"))
                .expect("Implement arguments should be valid"),
            vec![
                "--model",
                "gpt-6-luna",
                "-c",
                "model_reasoning_effort=xhigh"
            ]
        );
    }

    #[test]
    fn launch_gate_command_waits_on_the_machine_socket_before_exporting_or_execing_agent() {
        let state_file = Path::new("/home/runner/.local/state/ai-mission-manager/runs/run-23.json");
        let launch = AgentLaunchContext {
            run_id: 23,
            state_file,
            agent: AgentKind::Codex,
            model: Some("gpt-6-luna"),
            effort: Some("xhigh"),
        };
        let command = build_gated_agent_command(
            "mission-socket",
            "mission-launch-23-session-4",
            Path::new("/opt/codex"),
            "Implement the issue",
            &launch,
        )
        .expect("launch command should be valid");
        let title = "'tmux' '-f' '/dev/null' '-L' 'mission-socket' 'select-pane' '-T' 'codex' -t \"$TMUX_PANE\"";
        let gate = "'tmux' '-f' '/dev/null' '-L' 'mission-socket' 'wait-for' 'mission-launch-23-session-4'";
        let first_export = "export AI_MISSION_MANAGER_RUN_ID='23'";
        let agent = "exec '/opt/codex' '--model' 'gpt-6-luna' '-c' 'model_reasoning_effort=xhigh' 'Implement the issue'";
        assert!(command.starts_with(title));
        assert!(command.find(title).unwrap() < command.find(gate).unwrap());
        assert!(command.find(gate).unwrap() < command.find(first_export).unwrap());
        assert!(command.ends_with(agent));

        let release = build_tmux_wait_for_command(
            "tmux",
            "mission-socket",
            "mission-launch-23-session-4",
            true,
        )
        .expect("release command should be valid");
        assert_eq!(
            release,
            "'tmux' '-f' '/dev/null' '-L' 'mission-socket' 'wait-for' '-S' 'mission-launch-23-session-4'"
        );
    }

    #[test]
    fn control_snapshot_response_places_stream_barrier_between_notifications() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let barrier_events = Arc::clone(&events);
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        let pending_responses = Arc::new(Mutex::new(VecDeque::from([PendingControlCommand {
            id: 1,
            response_sender: Some(response_sender),
            after_end: Some(Box::new(move || {
                barrier_events
                    .lock()
                    .expect("event log should remain available")
                    .push("snapshot barrier".to_owned());
            })),
        }])));
        let (ready_sender, _ready_receiver) = mpsc::sync_channel(1);
        let output_events = Arc::clone(&events);
        let mut reader = TmuxControlReader::new(
            "%1".to_owned(),
            pending_responses,
            ready_sender,
            move |output| {
                output_events
                    .lock()
                    .expect("event log should remain available")
                    .push(format!("output:{}", String::from_utf8_lossy(&output)));
            },
            |_| {},
        );

        reader.handle_line(b"%output %1 before\\015\n");
        reader.handle_line(b"%begin 123 4 0\n");
        reader.handle_line(b"snapshot bytes\n");
        reader.handle_line(b"%end 123 4 0\n");
        reader.handle_line(b"%output %1 after\\015\n");

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("snapshot response should be delivered")
                .expect("snapshot command should succeed"),
            b"snapshot bytes\n"
        );
        assert_eq!(
            *events.lock().expect("event log should remain available"),
            vec!["output:before\r", "snapshot barrier", "output:after\r"]
        );
    }

    #[test]
    fn pane_snapshot_replays_rows_from_the_first_column_and_restores_the_cursor() {
        let capture = b"linha1\n  linha2\n\tlinha3\n\n\n";

        assert_eq!(
            render_pane_snapshot(capture, parse_pane_cursor(b"3 2\n")),
            b"linha1\r\n  linha2\r\n\tlinha3\x1b[0m\x1b[3;4H"
        );
        assert_eq!(
            render_pane_snapshot(capture, parse_pane_cursor(b"")),
            b"linha1\r\n  linha2\r\n\tlinha3\x1b[0m"
        );
    }

    #[test]
    fn control_snapshot_barrier_routes_post_sample_output_through_the_terminal_gate() {
        use crate::features::work::runs::{DeferredTerminalEvent, TerminalCallbackGate};

        let gate = Arc::new(TerminalCallbackGate::default());
        let events = Arc::new(Mutex::new(Vec::new()));
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        let barrier_gate = Arc::clone(&gate);
        let pending_responses = Arc::new(Mutex::new(VecDeque::from([PendingControlCommand {
            id: 1,
            response_sender: Some(response_sender),
            after_end: Some(Box::new(move || barrier_gate.mark_snapshot_captured())),
        }])));
        let (ready_sender, _ready_receiver) = mpsc::sync_channel(1);
        let output_gate = Arc::clone(&gate);
        let output_events = Arc::clone(&events);
        let mut reader = TmuxControlReader::new(
            "%1".to_owned(),
            pending_responses,
            ready_sender,
            move |output| {
                output_gate.dispatch_output_or_queue(output, |output| {
                    output_events
                        .lock()
                        .expect("event log should remain available")
                        .push(format!("output:{}", String::from_utf8_lossy(&output)));
                });
            },
            |_| {},
        );

        reader.handle_line(b"%output %1 before-sample\\015\n");
        reader.handle_line(b"%begin 123 4 0\n");
        reader.handle_line(b"snapshot contains before-sample\n");
        reader.handle_line(b"%end 123 4 0\n");
        reader.handle_line(b"%output %1 after-sample\\015\n");

        let snapshot = response_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("snapshot response should arrive")
            .expect("snapshot capture should succeed");
        assert_eq!(snapshot, b"snapshot contains before-sample\n");
        let _event_dispatch_guard = gate.lock_terminal_event_dispatch();
        let (state_records, pending_events) = gate.activate_and_drain();
        assert!(state_records.is_empty());
        for event in pending_events {
            match event {
                DeferredTerminalEvent::Output(output) => events
                    .lock()
                    .expect("event log should remain available")
                    .push(format!("output:{}", String::from_utf8_lossy(&output))),
                DeferredTerminalEvent::Exit(code) => events
                    .lock()
                    .expect("event log should remain available")
                    .push(format!("exit:{code:?}")),
            }
        }
        assert_eq!(
            *events.lock().expect("event log should remain available"),
            vec!["output:after-sample\r"]
        );
    }

    #[test]
    fn local_agent_launch_returns_a_stable_tmux_pane_identity() {
        let directory = tempdir().expect("temporary launch directory should exist");
        let machine = Machine {
            id: 1,
            context_id: 1,
            name: "Test Mac".into(),
            socket_name: format!("ai-mission-manager-test-{}", std::process::id()),
            transport: MachineTransport::Local,
            last_observed: crate::domain::MachineObservation::Unknown,
            last_observed_at: None,
        };
        let session_name = format!("mission-manager-test-{}", std::process::id());
        let pane_id = TmuxRuntime
            .launch_agent(
                &machine,
                &session_name,
                &format!("launch-test-{}", std::process::id()),
                directory.path(),
                Path::new("/bin/sleep"),
                "30",
                AgentLaunchContext {
                    run_id: 1,
                    state_file: &directory.path().join("state.json"),
                    agent: AgentKind::Claude,
                    model: None,
                    effort: None,
                },
            )
            .expect("tmux should launch the test process");

        TmuxRuntime
            .release_agent_launch(&machine, &format!("launch-test-{}", std::process::id()))
            .expect("tmux should release the test process");

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
            transport: MachineTransport::Local,
            last_observed: crate::domain::MachineObservation::Unknown,
            last_observed_at: None,
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
            &pane
                .capture_pane_snapshot(|| {})
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

    #[test]
    fn external_terminal_command_checks_the_exact_local_pane_before_attaching() {
        let command = build_pane_attach_command(&ExternalPaneIdentity {
            tmux_path: "tmux".into(),
            socket_name: "mission-manager".into(),
            session_name: "mission-item-1-run-2".into(),
            pane_id: "%7".into(),
            transport: TerminalTransport::Local,
        })
        .expect("the local Pane identity should build");

        assert!(command.contains("display-message"));
        assert!(command.contains("-t' '%7"));
        assert!(command.contains("if [ -z \"$actual_session\" ]; then"));
        assert!(command.contains("attach-session' '-t' '%7"));
        assert!(command.contains("exec 'tmux'"));
    }

    #[test]
    fn external_terminal_command_uses_ssh_for_a_remote_pane() {
        let command = build_pane_attach_command(&ExternalPaneIdentity {
            tmux_path: "/usr/bin/tmux".into(),
            socket_name: "mission-manager".into(),
            session_name: "remote-run".into(),
            pane_id: "%12".into(),
            transport: TerminalTransport::Ssh(SshTransport {
                host: "remote.example".into(),
                user: Some("runner".into()),
                port: Some(2222),
                identity_file: Some("/Users/me/.ssh/mission".into()),
                known_hosts_file: Some("/Users/me/.ssh/known_hosts".into()),
                strict_host_key_checking: Some("accept-new".into()),
                ssh_path: Some("/usr/bin/ssh".into()),
            }),
        })
        .expect("the remote Pane identity should build");

        assert!(command.starts_with("'/usr/bin/ssh' '-tt' '-o' 'BatchMode=yes'"));
        assert!(command.contains("'-p' '2222'"));
        assert!(command.contains("'runner@remote.example'"));
        assert!(command.contains("/usr/bin/tmux"));
        assert!(command.contains("display-message"));
        assert!(command.contains("attach-session"));
        assert!(command.contains("%12"));
        assert!(!command.starts_with("'tmux'"));
    }

    #[test]
    fn external_terminal_command_rejects_a_non_pane_identity() {
        let error = build_pane_attach_command(&ExternalPaneIdentity {
            tmux_path: "tmux".into(),
            socket_name: "mission-manager".into(),
            session_name: "mission-item-1-run-2".into(),
            pane_id: "main".into(),
            transport: TerminalTransport::Local,
        })
        .expect_err("a display name must not be accepted as a Pane identity");

        assert_eq!(error, "invalid tmux Pane identity: main");
    }

    #[test]
    fn machine_pane_query_includes_the_agent_state_option() {
        assert_eq!(
            machine_pane_query_format(),
            format!("#{{session_name}}\t#{{pane_id}}\t#{{{AGENT_STATE_OPTION}}}")
        );
    }

    #[test]
    fn state_file_read_command_batches_deduplicated_runs_over_ssh() {
        let command = build_agent_state_files_read_command(&[9, 7, 9, 0, -1])
            .expect("valid Run IDs should create one batch command");

        assert!(command.contains("state_dir=\"$HOME/.local/state/ai-mission-manager/runs\""));
        assert!(command.contains("for run_id in 7 9; do"));
        assert_eq!(command.matches("cat \"$state_file\"").count(), 1);
        assert!(command.contains("printf '%s\\000' \"$run_id\""));
        assert!(command.contains("printf '\\000'"));

        let remote_machine = Machine {
            id: 9,
            context_id: 1,
            name: "Remote Machine".into(),
            socket_name: "mission".into(),
            transport: MachineTransport::Ssh {
                host: "build.example".into(),
                user: Some("runner".into()),
                port: Some(2222),
                identity_file: None,
                known_hosts_file: None,
                strict_host_key_checking: None,
            },
            last_observed: crate::domain::MachineObservation::Unknown,
            last_observed_at: None,
        };
        let (program, arguments) = build_machine_shell_command(&remote_machine, &command)
            .expect("the batched file command should use SSH");
        assert_eq!(program, "ssh");
        assert!(arguments
            .iter()
            .any(|argument| argument == "runner@build.example"));
        assert_eq!(arguments.last(), Some(&command));
    }

    #[test]
    fn batched_state_reader_preserves_multiline_json_and_ignores_truncated_tail() {
        let record = AgentStateRecord {
            agent: AgentKind::Claude,
            run_id: "7".into(),
            state: crate::domain::RunState::Blocked,
            updated_at: "2026-09-24T12:00:00Z".into(),
            sequence: Some(3),
        };
        let pretty_record = serde_json::to_string_pretty(&record)
            .expect("the state record should serialize as multiline JSON");
        let contents = format!("7\0{pretty_record}\0{}\0{}", 8, r#"{"state":"blocked"}"#);

        let records = parse_agent_state_files_read(&contents, &[7, 8]);

        assert_eq!(records, vec![record]);
    }

    #[test]
    fn remote_tmux_commands_use_ssh_without_a_local_fallback() {
        let command = build_tmux_process_command(
            &TerminalTransport::Ssh(SshTransport {
                host: "remote.example".into(),
                user: Some("runner".into()),
                port: Some(2222),
                identity_file: None,
                known_hosts_file: None,
                strict_host_key_checking: Some("yes".into()),
                ssh_path: Some("/usr/bin/ssh".into()),
            }),
            false,
            &["-f", "/dev/null", "-L", "mission-manager", "list-sessions"],
            "tmux",
        )
        .expect("the remote tmux command should build");

        assert_eq!(command.0, "/usr/bin/ssh");
        assert_eq!(&command.1[..4], &["-T", "-o", "BatchMode=yes", "-p"]);
        assert!(command
            .1
            .iter()
            .any(|argument| argument == "runner@remote.example"));
        assert!(command
            .1
            .last()
            .is_some_and(|argument| argument.contains("'tmux'")));
        assert!(!command.0.ends_with("tmux"));
    }

    #[test]
    fn remote_hook_commands_are_limited_to_owned_paths_and_write_atomically() {
        let remote_machine = Machine {
            id: 9,
            context_id: 1,
            name: "Remote Machine".into(),
            socket_name: "mission".into(),
            transport: MachineTransport::Ssh {
                host: "build.example".into(),
                user: Some("runner".into()),
                port: Some(2222),
                identity_file: Some("/Users/me/.ssh/mission".into()),
                known_hosts_file: Some("/Users/me/.ssh/known_hosts".into()),
                strict_host_key_checking: Some("yes".into()),
            },
            last_observed: crate::domain::MachineObservation::Unknown,
            last_observed_at: None,
        };
        let claude_read = build_remote_hook_read_command(AgentKind::Claude);
        assert!(claude_read.contains("$HOME/.claude/settings.json"));
        assert!(claude_read.contains("[ -L \"$target\" ]"));
        assert!(claude_read.contains("[ -e \"$target\" ] && [ ! -f \"$target\" ]"));
        assert!(claude_read.contains("printf '\\001'; cat"));
        assert!(!claude_read.contains(".codex/"));
        let (ssh_read_program, ssh_read_arguments) =
            build_machine_shell_command(&remote_machine, &claude_read)
                .expect("remote settings read should use SSH");
        assert_eq!(ssh_read_program, "ssh");
        assert!(ssh_read_arguments.contains(&"runner@build.example".into()));
        assert_eq!(ssh_read_arguments.last(), Some(&claude_read));

        let codex_read = build_remote_hook_read_command(AgentKind::Codex);
        assert!(codex_read.contains("$HOME/.codex/hooks.json"));
        assert!(!codex_read.contains(".claude/"));

        let claude_write = build_remote_hook_write_command(AgentKind::Claude);
        assert!(claude_write.contains("set -eu; umask 077;"));
        assert!(claude_write.contains("target=\"$HOME/.claude/settings.json\""));
        assert!(claude_write.contains("[ -L \"$target\" ]"));
        assert!(claude_write.contains("[ -e \"$target\" ] && [ ! -f \"$target\" ]"));
        assert!(claude_write.contains("temporary=\"$target.tmp.$$\""));
        assert!(claude_write.contains("(set -C; : > \"$temporary\")"));
        assert!(claude_write.contains("cmp -s \"$target\" \"$temporary\""));
        assert!(claude_write.contains("mv -f \"$temporary\" \"$target\""));
        assert!(claude_write.contains("[ ! -f \"$target\" ]; then printf"));
        assert!(claude_write.contains("chmod 600 \"$temporary\""));
        assert!(!claude_write.contains(".codex/"));
        let (ssh_write_program, ssh_write_arguments) =
            build_machine_shell_command(&remote_machine, &claude_write)
                .expect("remote settings write should use SSH");
        assert_eq!(ssh_write_program, "ssh");
        assert_eq!(ssh_write_arguments.last(), Some(&claude_write));

        let script_install = build_remote_hook_script_install_command();
        assert!(script_install.contains(&format!(
            "$HOME/{}",
            crate::agent_state::AGENT_STATE_HOOK_RELATIVE_PATH
        )));
        assert!(script_install.contains("cmp -s \"$target\" \"$temporary\""));
        assert!(script_install.contains("if [ ! -x \"$target\" ]; then chmod 700"));
        assert!(script_install.contains("chmod 700 \"$temporary\""));
        assert!(!script_install.contains(".claude/"));
        assert!(!script_install.contains(".codex/"));

        let state_probe = build_remote_state_directory_probe_command();
        assert!(state_probe.contains("$HOME/.local/state/ai-mission-manager/runs"));
        assert!(state_probe.contains("set -C; : > \"$temporary\""));
        assert!(state_probe.contains("rm -f \"$temporary\""));
        assert!(!state_probe.contains(".ssh/"));
        assert_eq!(
            state_file_for_machine_run(Path::new("/home/runner"), 23),
            PathBuf::from("/home/runner/.local/state/ai-mission-manager/runs/run-23.json")
        );
    }

    #[test]
    fn fake_remote_provisioning_is_idempotent_and_preserves_user_hooks() {
        let machine = Machine {
            id: 9,
            context_id: 1,
            name: "Remote Machine".into(),
            socket_name: "mission".into(),
            transport: MachineTransport::Ssh {
                host: "build.example".into(),
                user: Some("runner".into()),
                port: None,
                identity_file: None,
                known_hosts_file: None,
                strict_host_key_checking: None,
            },
            last_observed: crate::domain::MachineObservation::Unknown,
            last_observed_at: None,
        };
        let fake = FakeTerminalRuntime::new([(machine.id, FakeMachineOutcome::Available)]);
        fake.set_agent_hook_config(
            machine.id,
            AgentKind::Claude,
            r#"{"permissions":{"allow":["Bash(*)"]},"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-claude-hook"}]},{"hooks":[{"type":"command","command":"'/old/.local/share/ai-mission-manager/hook.sh' working claude"}]}]}}"#,
        );
        fake.set_agent_hook_config(
            machine.id,
            AgentKind::Codex,
            r#"{"model":"gpt-5","hooks":{"PermissionRequest":[{"hooks":[{"type":"command","command":"user-codex-hook"}]}]}}"#,
        );

        let first = fake.preflight_agent_run(&machine, AgentKind::Claude, 23, None);
        assert_eq!(first.readiness.claude_hooks.current, Some(true));
        assert_eq!(first.readiness.codex_hooks.current, Some(true));
        assert_eq!(
            first.state_file,
            Some(PathBuf::from(
                "/fake/home/.local/state/ai-mission-manager/runs/run-23.json"
            ))
        );
        let claude_after_first = fake
            .agent_hook_config(machine.id, AgentKind::Claude)
            .unwrap();
        let codex_after_first = fake
            .agent_hook_config(machine.id, AgentKind::Codex)
            .unwrap();

        let second = fake.preflight_agent_run(&machine, AgentKind::Claude, 23, None);
        assert_eq!(second.readiness.claude_hooks.current, Some(true));
        assert_eq!(second.readiness.codex_hooks.current, Some(true));
        assert_eq!(
            fake.agent_hook_config(machine.id, AgentKind::Claude)
                .as_deref(),
            Some(claude_after_first.as_str())
        );
        assert_eq!(
            fake.agent_hook_config(machine.id, AgentKind::Codex)
                .as_deref(),
            Some(codex_after_first.as_str())
        );

        let claude: serde_json::Value = serde_json::from_str(&claude_after_first).unwrap();
        let codex: serde_json::Value = serde_json::from_str(&codex_after_first).unwrap();
        assert_eq!(claude["permissions"]["allow"][0], "Bash(*)");
        assert_eq!(
            claude["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "user-claude-hook"
        );
        assert!(claude["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|group| group["hooks"].as_array().into_iter().flatten())
            .any(|hook| {
                hook["command"] == "'/old/.local/share/ai-mission-manager/hook.sh' working claude"
            }));
        assert_eq!(codex["model"], "gpt-5");
        assert_eq!(
            codex["hooks"]["PermissionRequest"][0]["hooks"][0]["command"],
            "user-codex-hook"
        );
        for (agent, settings) in [(AgentKind::Claude, &claude), (AgentKind::Codex, &codex)] {
            for (event, _) in agent_state::agent_hook_events(agent) {
                let app_hooks = settings["hooks"][*event]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|group| group["hooks"].as_array().into_iter().flatten())
                    .filter(|hook| {
                        hook["command"].as_str().is_some_and(|command| {
                            command.contains("--agent-state-hook")
                                || command
                                    .contains(crate::agent_state::AGENT_STATE_HOOK_RELATIVE_PATH)
                        })
                    })
                    .count();
                assert_eq!(app_hooks, 1, "{agent:?} {event} should have one app hook");
            }
        }
    }

    #[test]
    fn agent_pane_discovery_recognizes_supported_cli_commands_only() {
        let claude = parse_agent_pane_summary(
            "manual-session\t%7\t/usr/local/bin/claude\tterminal\t/tmp/working-directory/service",
        )
        .expect("the Pane description should parse")
        .expect("Claude should be recognized");
        assert_eq!(claude.agent, AgentKind::Claude);
        assert_eq!(claude.session_name, "manual-session");
        assert_eq!(claude.pane_id, "%7");

        let codex =
            parse_agent_pane_summary("manual-session\t%8\tcodex\tterminal\t/tmp/working-directory")
                .expect("the Pane description should parse")
                .expect("Codex should be recognized");
        assert_eq!(codex.agent, AgentKind::Codex);

        let gated = parse_agent_pane_summary(
            "mission-item-3-run-9\t%9\tsh\tclaude\t/tmp/working-directory",
        )
        .expect("the gated Pane description should parse")
        .expect("the agent title should identify the waiting shell");
        assert_eq!(gated.agent, AgentKind::Claude);

        assert!(
            parse_agent_pane_summary("shell\t%10\tbash\tterminal\t/tmp/working-directory")
                .expect("the Pane description should parse")
                .is_none()
        );
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
