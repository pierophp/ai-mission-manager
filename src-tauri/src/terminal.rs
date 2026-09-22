use std::{
    env,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;

use crate::{
    agent_state::{AgentStateRecord, AGENT_STATE_OPTION},
    domain::{AgentKind, Machine, MachineTransport},
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

    fn kill_pane(&self, machine: &Machine, session_name: &str, pane_id: &str)
        -> Result<(), String>;
}

pub struct AgentLaunchContext<'a> {
    pub run_id: i64,
    pub state_file: &'a Path,
    pub agent: Option<AgentKind>,
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

    fn kill_pane(
        &self,
        machine: &Machine,
        _session_name: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        kill_tmux_pane(machine, pane_id)
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
            let output = run_machine_shell(machine, &format!("command -v {}", shell_quote(name)))?;
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
    let output = match &machine.transport {
        MachineTransport::Local => Command::new("sh")
            .args(["-lc", command])
            .output()
            .map_err(|error| format!("Could not inspect Machine {}: {error}", machine.name))?,
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
            Command::new(transport.ssh_path.as_deref().unwrap_or("ssh"))
                .args(arguments)
                .output()
                .map_err(|error| {
                    format!("Could not connect to Machine {}: {error}", machine.name)
                })?
        }
    };
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
    let mut command = format!(
        "export AI_MISSION_MANAGER_RUN_ID={}; export AI_MISSION_MANAGER_STATE_FILE={}; export AI_MISSION_MANAGER_TMUX_PATH=tmux; export AI_MISSION_MANAGER_TMUX_SOCKET={}; export AI_MISSION_MANAGER_PANE_ID=\"$TMUX_PANE\"; exec {}",
        shell_quote(&launch.run_id.to_string()),
        shell_quote(&launch.state_file.to_string_lossy()),
        shell_quote(&machine.socket_name),
        shell_quote(&executable.to_string_lossy()),
    );
    if let Some(agent) = launch.agent {
        for argument in agent_cli_arguments(agent, launch.model, launch.effort)? {
            command.push(' ');
            command.push_str(&shell_quote(&argument));
        }
    }
    command.push(' ');
    command.push_str(&shell_quote(prompt));
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
                "codex-luna",
                "-c",
                "model_reasoning_effort=xhigh"
            ]
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
                directory.path(),
                Path::new("/bin/sleep"),
                "30",
                AgentLaunchContext {
                    run_id: 1,
                    state_file: &directory.path().join("state.json"),
                    agent: None,
                    model: None,
                    effort: None,
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

        assert!(
            parse_agent_pane_summary("shell\t%9\tbash\tterminal\t/tmp/working-directory")
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
