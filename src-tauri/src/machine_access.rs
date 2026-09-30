//! Shell and file access for local and SSH Machines.
//!
//! Features depend on this small seam instead of choosing a transport. The
//! production adapter delegates process and file operations to the existing
//! transport implementation; tests can supply a fake without invoking SSH.

use std::path::{Path, PathBuf};

use crate::domain::{Machine, MachineTransport};

mod local_ssh;

#[cfg(test)]
pub(crate) use local_ssh::SshTransport;
#[cfg(test)]
pub(crate) use local_ssh::{
    build_machine_shell_command, build_remote_pstack_tree_command, encode_pstack_tree,
    write_local_pstack_tree,
};
pub(crate) use local_ssh::{
    build_tmux_process_command, find_agent_executable, is_executable, machine_home,
    probe_local_runtime, probe_machine, provision_pstack_tree, run_machine_shell,
    run_machine_shell_with_input, shell_quote, terminal_transport, validate_ssh_transport,
    write_machine_file, TerminalTransport,
};

pub(crate) trait MachineAccess: Send + Sync {
    fn run_shell(&self, machine: &Machine, command: &str) -> Result<String, String>;

    fn find_executable(&self, machine: &Machine, name: &str) -> Result<PathBuf, String>;

    fn home_directory(&self, machine: &Machine) -> Result<PathBuf, String>;

    fn home_path(&self, machine: &Machine) -> String;

    fn is_local(&self, machine: &Machine) -> bool;

    fn resolve_path(&self, machine: &Machine, path: &str) -> PathBuf;

    fn write_file(&self, machine: &Machine, path: &Path, contents: &[u8]) -> Result<(), String>;

    fn provision_pstack_tree(&self, machine: &Machine) -> Result<String, String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LocalSshMachineAccess;

impl MachineAccess for LocalSshMachineAccess {
    fn run_shell(&self, machine: &Machine, command: &str) -> Result<String, String> {
        run_machine_shell(machine, command)
    }

    fn find_executable(&self, machine: &Machine, name: &str) -> Result<PathBuf, String> {
        find_agent_executable(machine, name)
    }

    fn home_directory(&self, machine: &Machine) -> Result<PathBuf, String> {
        machine_home(machine)
    }

    fn home_path(&self, machine: &Machine) -> String {
        match &machine.transport {
            MachineTransport::Local => std::env::var("HOME").unwrap_or_else(|_| "/".into()),
            MachineTransport::Ssh { .. } => "~".into(),
        }
    }

    fn is_local(&self, machine: &Machine) -> bool {
        matches!(machine.transport, MachineTransport::Local)
    }

    fn resolve_path(&self, machine: &Machine, path: &str) -> PathBuf {
        match &machine.transport {
            MachineTransport::Local => resolve_path(path, &self.home_path(machine)),
            MachineTransport::Ssh { .. } => PathBuf::from(path),
        }
    }

    fn write_file(&self, machine: &Machine, path: &Path, contents: &[u8]) -> Result<(), String> {
        write_machine_file(machine, path, contents)
    }

    fn provision_pstack_tree(&self, machine: &Machine) -> Result<String, String> {
        provision_pstack_tree(machine)
    }
}

fn resolve_path(path: &str, machine_home: &str) -> PathBuf {
    if path == "~" {
        return PathBuf::from(machine_home);
    }
    if let Some(relative) = path.strip_prefix("~/") {
        return Path::new(machine_home).join(relative);
    }
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else {
        Path::new(machine_home).join(path)
    }
}

/// Quotes one argument for a POSIX shell command built by a feature.
pub(crate) fn quote_shell_argument(value: &str) -> String {
    shell_quote(value)
}

#[cfg(test)]
mod fake;

#[cfg(test)]
pub(crate) use fake::{FakeMachineAccess, FakeMachineAccessCall};
