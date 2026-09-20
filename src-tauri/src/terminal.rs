use std::{path::Path, process::Command};

use crate::domain::Machine;

pub trait TerminalRuntime {
    fn launch_agent(
        &self,
        machine: &Machine,
        session_name: &str,
        root: &Path,
        executable: &Path,
        prompt: &str,
    ) -> Result<String, String>;

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String>;
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
    ) -> Result<String, String> {
        launch_tmux_agent(machine, session_name, root, executable, prompt)
    }

    fn kill_session(&self, machine: &Machine, session_name: &str) -> Result<(), String> {
        kill_tmux_session(machine, session_name)
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_tmux(machine: &Machine, args: &[String]) -> Result<String, String> {
    let output = Command::new("tmux")
        .args(["-f", "/dev/null", "-L"])
        .arg(&machine.socket_name)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start tmux: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            format!("tmux exited with {}", output.status)
        } else {
            detail
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn launch_tmux_agent(
    machine: &Machine,
    session_name: &str,
    root: &Path,
    executable: &Path,
    prompt: &str,
) -> Result<String, String> {
    let command = format!(
        "exec {} {}",
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
}
