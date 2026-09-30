//! Local and SSH implementations of Machine access.

use std::{
    env,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::domain::{Machine, MachineTransport};

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

pub(crate) fn validate_ssh_transport(transport: &SshTransport) -> Result<(), String> {
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

pub(crate) fn build_tmux_process_command(
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

pub(crate) fn is_executable(path: &Path) -> bool {
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

pub(crate) fn provision_pstack_tree(machine: &Machine) -> Result<String, String> {
    let home = machine_home(machine)?;
    let target = crate::pstack::tree_directory(&home);
    match machine.transport {
        MachineTransport::Local => {
            write_local_pstack_tree(
                &target,
                crate::pstack::PSTACK_TREE_HASH,
                crate::pstack::PSTACK_TREE,
            )?;
        }
        MachineTransport::Ssh { .. } => {
            let target_text = target.to_string_lossy();
            let command = build_remote_pstack_tree_command(&target_text);
            let input = encode_pstack_tree();
            run_machine_shell_with_input(machine, &command, &input).map_err(|error| {
                format!(
                    "Could not provision pstack on Machine {}: {error}",
                    machine.name
                )
            })?;
        }
    }
    Ok(target.to_string_lossy().into_owned())
}

pub(crate) fn write_local_pstack_tree(
    target: &Path,
    tree_hash: &str,
    files: &[(&str, &[u8], bool)],
) -> Result<(), String> {
    if target.is_dir() {
        return Ok(());
    }
    if target.exists() {
        return Err(format!(
            "pstack target exists but is not a directory: {}",
            target.display()
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| "pstack tree path has no parent".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create pstack directory: {error}"))?;
    let temporary = parent.join(format!(".pstack-{tree_hash}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    for (relative, contents, executable) in files {
        if Path::new(relative).is_absolute()
            || Path::new(relative)
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!("Invalid embedded pstack path: {relative}"));
        }
        let file = temporary.join(relative);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create pstack subdirectory: {error}"))?;
        }
        std::fs::write(&file, contents)
            .map_err(|error| format!("Could not write pstack file {}: {error}", file.display()))?;
        #[cfg(unix)]
        std::fs::set_permissions(
            &file,
            std::os::unix::fs::PermissionsExt::from_mode(if *executable { 0o755 } else { 0o644 }),
        )
        .map_err(|error| {
            format!(
                "Could not set pstack file permissions {}: {error}",
                file.display()
            )
        })?;
    }
    match std::fs::rename(&temporary, target) {
        Ok(()) => Ok(()),
        Err(_error) if target.is_dir() => {
            let _ = std::fs::remove_dir_all(temporary);
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_dir_all(temporary);
            Err(format!("Could not install pstack tree: {error}"))
        }
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

pub(crate) fn encode_pstack_tree() -> Vec<u8> {
    let mut encoded = Vec::new();
    for (path, contents, executable) in crate::pstack::PSTACK_TREE {
        encoded.extend_from_slice(path.as_bytes());
        encoded.push(b'\n');
        encoded.extend_from_slice(if *executable { b"1\n" } else { b"0\n" });
        encoded.extend_from_slice(encode_base64(contents).as_bytes());
        encoded.push(b'\n');
    }
    encoded.push(b'\n');
    encoded
}

pub(crate) fn build_remote_pstack_tree_command(target: &str) -> String {
    let target = shell_quote(target);
    format!("set -eu; target={target}; if [ -d \"$target\" ]; then exit 0; fi; parent=${{target%/*}}; mkdir -p \"$parent\"; temporary=\"$target.tmp.$$\"; trap 'rm -rf \"$temporary\"' EXIT HUP INT TERM; mkdir -p \"$temporary\"; while IFS= read -r relative && [ -n \"$relative\" ]; do IFS= read -r executable || exit 1; IFS= read -r contents || exit 1; case \"$relative\" in /*|*..*) exit 1;; esac; file=\"$temporary/$relative\"; mkdir -p \"${{file%/*}}\"; printf '%s' \"$contents\" | base64 -d > \"$file\"; if [ \"$executable\" = 1 ]; then chmod 755 \"$file\"; fi; done; if [ -d \"$target\" ]; then exit 0; fi; mv \"$temporary\" \"$target\"; trap - EXIT HUP INT TERM")
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn find_agent_executable(machine: &Machine, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(format!("unsupported agent executable name: {name}"));
    }
    match &machine.transport {
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

pub(crate) fn run_machine_shell_with_input(
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

pub(crate) fn build_machine_shell_command(
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

pub(crate) fn machine_home(machine: &Machine) -> Result<PathBuf, String> {
    match &machine.transport {
        MachineTransport::Local => env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set on the local Machine".to_owned()),
        MachineTransport::Ssh { .. } => run_machine_shell(machine, "printf '%s' \"$HOME\"")
            .and_then(|home| {
                let home = PathBuf::from(home.trim());
                if home.is_absolute() {
                    Ok(home)
                } else {
                    Err(format!(
                        "Machine {} did not report an absolute home directory",
                        machine.name
                    ))
                }
            }),
    }
}

pub(crate) fn write_machine_file(
    machine: &Machine,
    path: &Path,
    contents: &[u8],
) -> Result<(), String> {
    match &machine.transport {
        MachineTransport::Local => {
            let parent = path
                .parent()
                .ok_or_else(|| "Generated role file has no parent directory".to_owned())?;
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create pstack role directory: {error}"))?;
            let temporary = path.with_extension(format!("md.tmp.{}", std::process::id()));
            std::fs::write(&temporary, contents)
                .map_err(|error| format!("Could not write pstack role file: {error}"))?;
            #[cfg(unix)]
            std::fs::set_permissions(
                &temporary,
                std::os::unix::fs::PermissionsExt::from_mode(0o600),
            )
            .map_err(|error| format!("Could not secure pstack role file: {error}"))?;
            std::fs::rename(&temporary, path)
                .map_err(|error| format!("Could not install pstack role file: {error}"))
        }
        MachineTransport::Ssh { .. } => {
            let target = shell_quote(&path.to_string_lossy());
            let command = format!("set -eu; umask 077; target={target}; parent=${{target%/*}}; mkdir -p \"$parent\"; temporary=\"$target.tmp.$$\"; trap 'rm -f \"$temporary\"' EXIT HUP INT TERM; cat > \"$temporary\"; chmod 600 \"$temporary\"; mv -f \"$temporary\" \"$target\"; trap - EXIT HUP INT TERM");
            run_machine_shell_with_input(machine, &command, contents).map(|_| ())
        }
    }
}
