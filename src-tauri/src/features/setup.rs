//! Setup feature implementation.
//!
//! This module owns the first-launch state, setup completion, and the health
//! report consumed by the setup workflow. The external probes remain adapters
//! (`dependencies`, `provider`, and `terminal`); this feature assembles their
//! results into the setup interface.

use std::sync::Mutex;

use tauri::State;

use crate::{
    app::Runtime,
    dependencies::{check_command, DependencyState, DependencyStatus},
    domain,
    terminal::probe_local_runtime,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderChoice {
    GitHub,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub completed: bool,
    pub provider: ProviderChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub runtime: DependencyStatus,
    pub provider: DependencyStatus,
    pub agents: Vec<DependencyStatus>,
    pub checked_at: i64,
}

pub(crate) fn list_grill_model_catalog() -> Vec<domain::GrillAgentCatalog> {
    domain::grill_model_catalog()
}

pub(crate) fn get_setup_state(state: State<'_, Mutex<Runtime>>) -> Result<SetupState, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .setup_state()
}

pub(crate) fn complete_setup(
    state: State<'_, Mutex<Runtime>>,
    context_name: String,
    provider: ProviderChoice,
) -> Result<SetupState, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .complete_setup(context_name, provider)
}

pub(crate) fn get_health_status(
    state: State<'_, Mutex<Runtime>>,
    provider: Option<ProviderChoice>,
) -> Result<HealthStatus, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .health_status(provider)
}

impl Runtime {
    pub(crate) fn setup_state(&self) -> Result<SetupState, String> {
        let completed = self
            .store
            .setting("setup_completed")
            .map_err(|error| error.to_string())?
            .as_deref()
            == Some("true");
        let provider = match self
            .store
            .setting("provider_choice")
            .map_err(|error| error.to_string())?
            .as_deref()
        {
            Some("github") | None => ProviderChoice::GitHub,
            Some("none") => ProviderChoice::None,
            Some(other) => return Err(format!("Unknown provider choice: {other}")),
        };
        Ok(SetupState {
            completed,
            provider,
        })
    }

    pub(crate) fn complete_setup(
        &mut self,
        context_name: String,
        provider: ProviderChoice,
    ) -> Result<SetupState, String> {
        let context_name = context_name.trim();
        if context_name.is_empty() {
            return Err("A Context name is required to finish setup".into());
        }
        if !self
            .state
            .contexts
            .iter()
            .any(|context| context.name == context_name)
        {
            self.create_context(context_name.to_owned())?;
        }
        self.store
            .set_setting("setup_completed", "true")
            .and_then(|_| {
                self.store.set_setting(
                    "provider_choice",
                    match provider {
                        ProviderChoice::GitHub => "github",
                        ProviderChoice::None => "none",
                    },
                )
            })
            .map_err(|error| error.to_string())?;
        Ok(SetupState {
            completed: true,
            provider,
        })
    }

    pub(crate) fn health_status(
        &mut self,
        provider_override: Option<ProviderChoice>,
    ) -> Result<HealthStatus, String> {
        let runtime = self.check_runtime_dependency()?;
        let setup = self.setup_state()?;
        let provider = match provider_override.unwrap_or(setup.provider) {
            ProviderChoice::GitHub => self.check_github_dependency()?,
            ProviderChoice::None => DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::NotConfigured,
                executable_path: None,
                message: "No provider selected; local Items remain available.".into(),
                action: Some(
                    "Choose GitHub in setup when you are ready to link external work.".into(),
                ),
            },
        };
        let agents = [
            ("claude", "Claude Code", "claude_executable_path"),
            ("codex", "Codex", "codex_executable_path"),
        ]
        .into_iter()
        .map(|(key, label, setting_key)| {
            self.check_local_dependency(
                key,
                label,
                setting_key,
                &[],
                &format!("Install {label} before starting a Run with it."),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
        Ok(HealthStatus {
            runtime,
            provider,
            agents,
            checked_at: crate::app::current_unix_seconds(),
        })
    }

    fn check_runtime_dependency(&mut self) -> Result<DependencyStatus, String> {
        let path = self.resolve_and_store_executable("tmux", "tmux_executable_path")?;
        let Some(path) = path else {
            return Ok(DependencyStatus {
                key: "tmux".into(),
                label: "tmux runtime".into(),
                state: DependencyState::Missing,
                executable_path: None,
                message: "tmux was not found.".into(),
                action: Some(
                    "Install tmux (for example, with `brew install tmux`) and check again.".into(),
                ),
            });
        };
        match probe_local_runtime(&path) {
            Ok(()) => Ok(DependencyStatus {
                key: "tmux".into(),
                label: "tmux runtime".into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: "tmux is ready.".into(),
                action: None,
            }),
            Err(detail) => Ok(DependencyStatus {
                key: "tmux".into(),
                label: "tmux runtime".into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("tmux could not be checked: {detail}"),
                action: Some("Repair or reinstall tmux, then check again.".into()),
            }),
        }
    }

    fn check_github_dependency(&mut self) -> Result<DependencyStatus, String> {
        let path = self.resolve_and_store_executable("gh", "gh_executable_path")?;
        let Some(path) = path else {
            return Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Missing,
                executable_path: None,
                message: "GitHub CLI (`gh`) was not found.".into(),
                action: Some(
                    "Install GitHub CLI, then authenticate it with `gh auth login`.".into(),
                ),
            });
        };
        if let Err(detail) = check_command(&path, &["--version"]) {
            return Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("GitHub CLI could not run: {detail}"),
                action: Some("Repair or reinstall GitHub CLI, then check again.".into()),
            });
        }
        match check_command(&path, &["auth", "status", "--hostname", "github.com"]) {
            Ok(()) => Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: "GitHub CLI is installed and authenticated.".into(),
                action: None,
            }),
            Err(detail) if looks_like_authentication_failure(&detail) => Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Unauthenticated,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("GitHub CLI is not authenticated: {detail}"),
                action: Some(
                    "Run `gh auth login` in your terminal; Mission Manager will not log in for you."
                        .into(),
                ),
            }),
            Err(detail) => Ok(DependencyStatus {
                key: "github".into(),
                label: "GitHub provider".into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("GitHub authentication status could not be checked: {detail}"),
                action: Some("Check network access to github.com, then check again.".into()),
            }),
        }
    }

    fn check_local_dependency(
        &mut self,
        key: &str,
        label: &str,
        setting_key: &str,
        args: &[&str],
        missing_action: &str,
    ) -> Result<DependencyStatus, String> {
        let path = self.resolve_and_store_executable(key, setting_key)?;
        let Some(path) = path else {
            return Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Missing,
                executable_path: None,
                message: format!("{label} was not found."),
                action: Some(missing_action.into()),
            });
        };
        if args.is_empty() {
            return Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("{label} is installed."),
                action: None,
            });
        }
        match check_command(&path, args) {
            Ok(()) => Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Available,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("{label} is ready."),
                action: None,
            }),
            Err(detail) => Ok(DependencyStatus {
                key: key.into(),
                label: label.into(),
                state: DependencyState::Unavailable,
                executable_path: Some(path.to_string_lossy().into_owned()),
                message: format!("{label} could not be checked: {detail}"),
                action: Some(missing_action.into()),
            }),
        }
    }
}

fn looks_like_authentication_failure(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    [
        "not logged in",
        "not authenticated",
        "no accounts",
        "authentication token",
        "token is invalid",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::*;
    use crate::dependencies::DependencyState;

    #[test]
    fn setup_completion_reuses_the_default_context_and_survives_reopening() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");

        assert!(
            !runtime
                .setup_state()
                .expect("setup state should be readable")
                .completed
        );
        let setup = runtime
            .complete_setup("Personal".into(), ProviderChoice::GitHub)
            .expect("setup should complete");

        assert!(setup.completed);
        assert_eq!(setup.provider, ProviderChoice::GitHub);
        assert_eq!(runtime.state.contexts.len(), 1);

        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(
            reopened
                .setup_state()
                .expect("setup state should survive reopening"),
            setup
        );
        assert_eq!(reopened.state.contexts.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn health_reports_an_unauthenticated_provider_without_hiding_a_healthy_runtime() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let tmux = directory.path().join("tmux");
        let gh = directory.path().join("gh");
        fs::write(&tmux, "#!/bin/sh\nprintf 'tmux 3.4\\n'\n").expect("fake tmux should be written");
        fs::write(
            &gh,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 0; fi\nprintf 'not logged in\\n' >&2\nexit 1\n",
        )
        .expect("fake gh should be written");
        for path in [&tmux, &gh] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("fake dependency should be executable");
        }

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .complete_setup("Personal".into(), ProviderChoice::GitHub)
            .expect("setup should complete");
        runtime
            .store
            .set_executable_path("tmux_executable_path", &tmux)
            .expect("tmux path should persist");
        runtime
            .store
            .set_executable_path("gh_executable_path", &gh)
            .expect("gh path should persist");

        let health = runtime
            .health_status(None)
            .expect("health checks should return independent statuses");

        assert_eq!(health.runtime.state, DependencyState::Available);
        assert_eq!(health.provider.state, DependencyState::Unauthenticated);
        assert!(Path::new(health.runtime.executable_path.as_deref().unwrap()).is_absolute());
        assert_eq!(
            runtime
                .store
                .executable_path("tmux_executable_path")
                .expect("tmux path should remain readable")
                .unwrap()
                .to_string_lossy(),
            health.runtime.executable_path.as_deref().unwrap()
        );
        assert!(health
            .provider
            .action
            .as_deref()
            .unwrap()
            .contains("gh auth login"));
    }
}
