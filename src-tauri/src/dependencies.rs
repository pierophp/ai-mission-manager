use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyState {
    Available,
    Missing,
    Unauthenticated,
    NotConfigured,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub key: String,
    pub label: String,
    pub state: DependencyState,
    pub executable_path: Option<String>,
    pub message: String,
    pub action: Option<String>,
}

pub fn resolve_executable(name: &str, stored_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = stored_path.filter(|path| path.is_absolute() && is_executable(path)) {
        return Some(
            path.canonicalize()
                .ok()
                .unwrap_or_else(|| path.to_path_buf()),
        );
    }

    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
        .and_then(|candidate| candidate.canonicalize().ok().or(Some(candidate)))
}

pub fn check_command(path: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new(path)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }

    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if detail.is_empty() {
        format!("command exited with {}", output.status)
    } else {
        detail
    })
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

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn discovered_executables_are_returned_as_absolute_paths() {
        let directory = tempdir().expect("temporary dependency directory should exist");
        let executable = directory.path().join("tmux");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("fake executable should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake executable should be executable");

        let path =
            resolve_executable("tmux", Some(&executable)).expect("executable should resolve");

        assert!(path.is_absolute());
        assert_eq!(
            path,
            executable.canonicalize().expect("path should canonicalize")
        );
    }

    #[test]
    fn command_failure_is_returned_without_affecting_other_dependency_checks() {
        let directory = tempdir().expect("temporary dependency directory should exist");
        let runtime = directory.path().join("tmux");
        let provider = directory.path().join("gh");
        fs::write(&runtime, "#!/bin/sh\nprintf 'tmux 3.4\\n'\n")
            .expect("runtime fixture should be written");
        fs::write(
            &provider,
            "#!/bin/sh\nprintf 'not logged in\\n' >&2\nexit 1\n",
        )
        .expect("provider fixture should be written");
        for path in [&runtime, &provider] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("fixture should be executable");
        }

        check_command(&runtime, &["-V"]).expect("runtime should be healthy");
        let provider_error = check_command(&provider, &["auth", "status"])
            .expect_err("provider authentication failure should be reported");

        assert_eq!(provider_error, "not logged in");
    }
}
