use super::*;

struct IssueCreationScope {
    item_id: i64,
    project_id: i64,
    context_id: i64,
    repository_id: i64,
    repository_project_id: i64,
    remote_url: String,
}

struct IssueCreationObservation {
    scope: IssueCreationScope,
    executable: PathBuf,
    created_url: String,
    object: ExternalObjectInput,
    snapshot: Option<crate::domain::ExternalSnapshotData>,
    warning: Option<String>,
}

impl Runtime {
    fn issue_creation_snapshot(
        &self,
        item_id: i64,
        repository_id: i64,
        title: &str,
    ) -> Result<(IssueCreationScope, Option<PathBuf>), String> {
        const UNAVAILABLE_SCOPE: &str =
            "The Item or Repository is not available in the Item's Project";
        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == item.project_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        if !self
            .state
            .contexts
            .iter()
            .any(|context| context.id == project.context_id)
        {
            return Err(UNAVAILABLE_SCOPE.into());
        }
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        let repository_project = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == repository.project_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        if repository.project_id != project.id
            || repository_project.context_id != project.context_id
        {
            return Err(UNAVAILABLE_SCOPE.into());
        }
        if github_repository_name(&repository.remote_url).is_none() {
            return Err("The selected Repository does not have a valid GitHub remote".into());
        }
        if title.trim().is_empty() {
            return Err("A GitHub Issue title is required".into());
        }
        Ok((
            IssueCreationScope {
                item_id,
                project_id: item.project_id,
                context_id: project.context_id,
                repository_id,
                repository_project_id: repository.project_id,
                remote_url: repository.remote_url.clone(),
            },
            self.gh_executable_path.clone(),
        ))
    }

    fn apply_issue_creation(
        &mut self,
        observation: IssueCreationObservation,
    ) -> Result<ExternalLinkAction, String> {
        let created_url = observation.created_url;
        let link_failure = |reason: String| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {reason}"
            )
        };
        if !self.issue_creation_scope_is_current(&observation.scope) {
            return Err(link_failure(
                "the Item or Repository changed while the Issue was being created".into(),
            ));
        }
        self.remember_gh_executable(&observation.executable)
            .map_err(link_failure)?;
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id: observation.scope.item_id,
                object: observation.object,
                snapshot: observation.snapshot,
            },
        )
        .map_err(|error| link_failure(error.to_string()))?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| link_failure("Issue creation produced no Link".into()))?;
        self.commit(decision).map_err(link_failure)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| link_failure("Issue creation produced no External Object".into()))?;
        Ok(ExternalLinkAction {
            link: view,
            warning: observation.warning,
        })
    }

    fn issue_creation_scope_is_current(&self, scope: &IssueCreationScope) -> bool {
        let Some(item) = self
            .state
            .items
            .iter()
            .find(|item| item.id == scope.item_id)
        else {
            return false;
        };
        let Some(project) = self
            .state
            .projects
            .iter()
            .find(|project| project.id == scope.project_id)
        else {
            return false;
        };
        let Some(repository) = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == scope.repository_id)
        else {
            return false;
        };
        item.project_id == scope.project_id
            && project.context_id == scope.context_id
            && repository.project_id == scope.repository_project_id
            && repository.project_id == scope.project_id
            && repository.remote_url == scope.remote_url
            && self
                .state
                .contexts
                .iter()
                .any(|context| context.id == scope.context_id)
    }

    fn remember_gh_executable(&mut self, executable: &Path) -> Result<(), String> {
        if self.gh_executable_path.as_deref() == Some(executable) {
            return Ok(());
        }
        self.store
            .set_gh_executable_path(executable)
            .map_err(|error| error.to_string())?;
        self.gh_executable_path = Some(executable.to_path_buf());
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::{create_github_issue_with_state, PollEntryObservation};
    use crate::{agent_state::AgentStateRecord, app::Runtime};

    #[test]
    fn slow_github_issue_creation_does_not_hold_the_runtime_lock() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let started = directory.path().join("gh-started");
        let release = directory.path().join("release-gh");
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = api ] && [ "$2" = repos/acme/app/issues ]; then
  touch '{}'
  attempts=0
  while [ ! -e '{}' ] && [ "$attempts" -lt 300 ]; do
    sleep 0.05
    attempts=$((attempts + 1))
  done
  [ -e '{}' ] || exit 1
  printf '%s' '{{"html_url":"https://github.com/acme/app/issues/42"}}'
  exit 0
fi
if [ "$1" = issue ] && [ "$2" = view ]; then
  printf '%s' '{{"number":42,"title":"Created from Mission Manager","state":"OPEN","author":null,"labels":[],"milestone":null,"updatedAt":null}}'
  exit 0
fi
exit 1
"#,
            started.display(),
            release.display(),
            release.display()
        );
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Issue target".into(), 1, 1)
            .expect("Item should be created");
        runtime
            .register_machine(
                1,
                "Local Mac".into(),
                "mission-test".into(),
                crate::domain::MachineTransport::Local,
            )
            .expect("Machine should be created");
        runtime.state.runs.push(crate::domain::Run {
            id: 7,
            item_id: 1,
            machine_id: 1,
            agent: crate::domain::AgentKind::Claude,
            execution_profile: crate::domain::ExecutionProfile::Implement,
            model: None,
            effort: None,
            skill_snapshot: None,
            prompt: "Implement the feature".into(),
            working_directory: directory.path().to_string_lossy().into_owned(),
            session_name: "run-7".into(),
            pane_id: "%7".into(),
            started_at: 1,
            state: crate::domain::RunState::Unknown,
            last_applied_agent_state_sequence: None,
            pane_status: crate::domain::RunPaneStatus::Unknown,
            workspace_id: None,
            repository_id: None,
            worktree_id: None,
            direct_checkouts: Vec::new(),
            transcript: String::new(),
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
            grill_action_started_at: None,
        });
        runtime
            .register_repository(
                1,
                "service".into(),
                "https://github.com/acme/app.git".into(),
            )
            .expect("Repository should be registered");
        runtime
            .store
            .set_gh_executable_path(&executable)
            .expect("fake gh path should persist");
        runtime.gh_executable_path = Some(executable);
        let state = Arc::new(Mutex::new(runtime));
        let worker_state = Arc::clone(&state);
        let worker = thread::spawn(move || {
            tauri::async_runtime::block_on(create_github_issue_with_state(
                1,
                1,
                "Created while local state changes".into(),
                "body".into(),
                worker_state.as_ref(),
            ))
        });

        let deadline = Instant::now() + Duration::from_secs(10);
        while !started.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(started.exists(), "fake GitHub CLI should have started");

        let lock_attempt = Instant::now();
        let local_state_change = state
            .lock()
            .expect("Runtime should remain accessible")
            .apply_agent_state_record(
                7,
                AgentStateRecord {
                    agent: crate::domain::AgentKind::Claude,
                    run_id: "7".into(),
                    state: crate::domain::RunState::Blocked,
                    updated_at: "123".into(),
                    sequence: Some(1),
                },
            )
            .expect("a local agent state report should process during the GitHub request");
        fs::write(&release, "continue")
            .expect("fake GitHub CLI should be released after the lock assertion");
        assert!(local_state_change.accepted);
        assert!(local_state_change.state_changed);
        assert!(
            lock_attempt.elapsed() < Duration::from_secs(1),
            "a slow GitHub call should not hold the Runtime lock"
        );

        let result = worker
            .join()
            .expect("GitHub issue worker should finish")
            .expect("Issue creation should still apply after a local change");
        assert_eq!(
            result.link.object.canonical_url,
            "https://github.com/acme/app/issues/42"
        );
        let runtime = state.lock().expect("Runtime should remain available");
        assert_eq!(runtime.state.items.len(), 1);
        assert_eq!(
            runtime.state.runs[0].state,
            crate::domain::RunState::Blocked
        );
        assert_eq!(runtime.state.links.len(), 1);
    }

    #[test]
    fn stale_refresh_and_poll_results_cannot_replace_a_newer_snapshot() {
        use crate::domain::{
            ExternalMetadata, ExternalObject, ExternalObjectKind, ExternalProvider,
            ExternalSnapshot, ExternalSnapshotData, Link,
        };

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        let object = ExternalObject {
            id: 1,
            provider: ExternalProvider::GitHub,
            kind: ExternalObjectKind::Issue,
            external_key: "acme/app#42".into(),
            canonical_url: "https://github.com/acme/app/issues/42".into(),
        };
        runtime.state.external_objects.push(object.clone());
        runtime.state.links.push(Link {
            id: 1,
            item_id: 1,
            external_object_id: object.id,
            reviewed_activity_id: 0,
            attention_policy: None,
            watch_until: None,
            review_at: None,
            is_spec: false,
            provenance: None,
        });
        let current = ExternalSnapshot {
            external_object_id: object.id,
            title: "Current Issue title".into(),
            state: "OPEN".into(),
            metadata: vec![ExternalMetadata {
                key: "number".into(),
                value: "42".into(),
            }],
            fetched_at: 20,
        };
        runtime.state.snapshots.push(current.clone());
        runtime.gh_executable_path = Some(executable.clone());
        runtime
            .external_snapshot_applied_generations
            .insert(object.id, 2);
        let stale = ExternalSnapshotData {
            title: "Older Issue title".into(),
            state: "CLOSED".into(),
            metadata: Vec::new(),
            fetched_at: 20,
        };

        let refresh_error = runtime
            .apply_refresh(&object, 1, executable.as_path(), stale.clone())
            .expect_err("an older refresh must not replace the current snapshot");
        assert!(refresh_error.contains("newer snapshot"));

        let poll_error = runtime
            .apply_poll_observation(PollEntryObservation {
                object: object.clone(),
                generation: 1,
                result: Ok(stale),
            })
            .expect_err("an older poll result must not replace the current snapshot");
        assert!(poll_error.contains("newer snapshot"));
        assert!(
            !runtime.external_snapshot_is_stale(object.id, 3, 20),
            "a newer request may apply a same-second result"
        );
        assert_eq!(runtime.state.snapshots, vec![current]);
        assert!(runtime.state.activities.is_empty());
    }
}

pub(crate) async fn create_github_issue_with_state(
    item_id: i64,
    repository_id: i64,
    title: String,
    body: String,
    state: &Mutex<Runtime>,
) -> Result<ExternalLinkAction, String> {
    let (scope, stored_executable) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.issue_creation_snapshot(item_id, repository_id, &title)?
    };
    let observation = tauri::async_runtime::spawn_blocking(move || {
        let executable = resolve_gh_executable(stored_executable.as_deref())
            .map_err(|error| error.to_string())?;
        let repository_name = github_repository_name(&scope.remote_url)
            .ok_or_else(|| "The selected Repository does not have a valid GitHub remote".to_owned())?;
        let github = GithubCli::new(executable.clone());
        let created_url = github
            .create_issue(&repository_name, title.trim(), &body)
            .map_err(|error| error.to_string())?;
        let object = classify_url(&created_url).map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue
        {
            return Err(format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: GitHub CLI returned a URL that is not a GitHub Issue"
            ));
        }
        let (snapshot, warning) = match github.fetch(&object, current_unix_seconds()) {
            Ok(snapshot) => (Some(snapshot), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(IssueCreationObservation {
            scope,
            executable,
            created_url,
            object,
            snapshot,
            warning,
        })
    })
    .await
    .map_err(|error| format!("GitHub Issue worker failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    runtime.apply_issue_creation(observation)
}

struct LinkSnapshot {
    item_id: i64,
    input: ExternalObjectInput,
    existing: Option<crate::domain::ExternalObject>,
    stored_executable: Option<PathBuf>,
}

struct LinkObservation {
    snapshot: LinkSnapshot,
    executable: Option<PathBuf>,
    fetched: Option<crate::domain::ExternalSnapshotData>,
    warning: Option<String>,
}

impl Runtime {
    fn link_snapshot(&self, item_id: i64, url: &str) -> Result<LinkSnapshot, String> {
        if !self.state.items.iter().any(|item| item.id == item_id) {
            return Err(format!("Item {item_id} does not exist"));
        }
        let input = classify_url(url).map_err(|error| error.to_string())?;
        let existing = self
            .state
            .external_objects
            .iter()
            .find(|object| {
                object.provider == input.provider && object.external_key == input.external_key
            })
            .cloned();
        Ok(LinkSnapshot {
            item_id,
            input,
            existing,
            stored_executable: self.gh_executable_path.clone(),
        })
    }

    fn link_snapshot_is_current(&self, snapshot: &LinkSnapshot) -> bool {
        if !self
            .state
            .items
            .iter()
            .any(|item| item.id == snapshot.item_id)
        {
            return false;
        }
        match &snapshot.existing {
            Some(previous) => self
                .state
                .external_objects
                .iter()
                .any(|current| current == previous),
            None => !self.state.external_objects.iter().any(|current| {
                current.provider == snapshot.input.provider
                    && current.external_key == snapshot.input.external_key
            }),
        }
    }

    fn apply_link_observation(
        &mut self,
        observation: LinkObservation,
    ) -> Result<ExternalLinkAction, String> {
        if !self.link_snapshot_is_current(&observation.snapshot) {
            return Err(
                "The Item or External Object changed while the link was being prepared; try again"
                    .into(),
            );
        }
        let mut warning = observation.warning;
        if let Some(executable) = observation.executable {
            if let Err(error) = self.remember_gh_executable(&executable) {
                warning.get_or_insert(error);
            }
        }
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id: observation.snapshot.item_id,
                object: observation.snapshot.input,
                snapshot: observation.fetched,
            },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| "Link creation produced no Link".to_owned())?;
        self.commit(decision)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| "Link creation produced no External Object".to_owned())?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    fn refresh_snapshot(
        &mut self,
        external_object_id: i64,
    ) -> Result<(crate::domain::ExternalObject, Option<PathBuf>, u64), String> {
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == external_object_id)
            .cloned()
            .ok_or_else(|| format!("External Object {external_object_id} does not exist"))?;
        if object.provider != ExternalProvider::GitHub {
            return Err("Only GitHub External Objects can be refreshed".into());
        }
        let generation = self.begin_external_snapshot_request(external_object_id);
        Ok((object, self.gh_executable_path.clone(), generation))
    }

    fn apply_refresh(
        &mut self,
        object: &crate::domain::ExternalObject,
        generation: u64,
        executable: &Path,
        snapshot: crate::domain::ExternalSnapshotData,
    ) -> Result<ExternalSnapshot, String> {
        if !self
            .state
            .external_objects
            .iter()
            .any(|current| current == object)
        {
            return Err(format!(
                "External Object {} changed while it was being refreshed; refresh it again",
                object.id
            ));
        }
        if self.external_snapshot_is_stale(object.id, generation, snapshot.fetched_at) {
            return Err(format!(
                "A newer snapshot for External Object {} was already applied; refresh it again",
                object.id
            ));
        }
        self.remember_gh_executable(executable)?;
        let decision = decide(
            self.state.clone(),
            Event::RefreshExternalObject {
                external_object_id: object.id,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let snapshot = decision
            .state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == object.id)
            .cloned()
            .ok_or_else(|| "Refresh produced no External snapshot".to_owned())?;
        self.commit(decision)?;
        self.external_snapshot_applied_generations
            .insert(object.id, generation);
        Ok(snapshot)
    }

    fn begin_external_snapshot_request(&mut self, external_object_id: i64) -> u64 {
        let generation = self
            .external_snapshot_request_generations
            .entry(external_object_id)
            .or_insert(0);
        *generation = generation
            .checked_add(1)
            .expect("External Object snapshot request generation should not overflow");
        *generation
    }

    fn external_snapshot_is_stale(
        &self,
        external_object_id: i64,
        generation: u64,
        fetched_at: i64,
    ) -> bool {
        self.state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == external_object_id)
            .is_some_and(|current| fetched_at < current.fetched_at)
            || self
                .external_snapshot_applied_generations
                .get(&external_object_id)
                .is_some_and(|current| generation <= *current)
    }

    fn apply_poll_observation(&mut self, observation: PollEntryObservation) -> Result<(), String> {
        let external_object_id = observation.object.id;
        if !self
            .state
            .external_objects
            .iter()
            .any(|current| current == &observation.object)
            || !self
                .state
                .links
                .iter()
                .any(|link| link.external_object_id == external_object_id)
        {
            return Err(
                "The External Object changed while it was being polled; poll it again".into(),
            );
        }
        let snapshot = observation.result?;
        if self.external_snapshot_is_stale(
            external_object_id,
            observation.generation,
            snapshot.fetched_at,
        ) {
            return Err(format!(
                "A newer snapshot for External Object {external_object_id} was already applied; poll it again"
            ));
        }
        let decision = decide(
            self.state.clone(),
            Event::RefreshExternalObject {
                external_object_id,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        self.commit(decision)?;
        self.external_snapshot_applied_generations
            .insert(external_object_id, observation.generation);
        Ok(())
    }
}

pub(crate) async fn link_external_object_with_state(
    item_id: i64,
    url: String,
    state: &Mutex<Runtime>,
) -> Result<ExternalLinkAction, String> {
    let snapshot = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.link_snapshot(item_id, &url)?
    };
    let observation = tauri::async_runtime::spawn_blocking(move || {
        let mut executable = None;
        let mut fetched = None;
        let mut warning = None;
        if snapshot.input.provider == ExternalProvider::GitHub && snapshot.existing.is_none() {
            match resolve_gh_executable(snapshot.stored_executable.as_deref()) {
                Ok(path) => {
                    match GithubCli::new(path.clone())
                        .fetch(&snapshot.input, current_unix_seconds())
                    {
                        Ok(value) => fetched = Some(value),
                        Err(error) => warning = Some(error.to_string()),
                    }
                    executable = Some(path);
                }
                Err(error) => warning = Some(error.to_string()),
            }
        }
        LinkObservation {
            snapshot,
            executable,
            fetched,
            warning,
        }
    })
    .await
    .map_err(|error| format!("External Object link worker failed: {error}"))?;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    runtime.apply_link_observation(observation)
}

pub(crate) async fn refresh_external_object_with_state(
    external_object_id: i64,
    state: &Mutex<Runtime>,
) -> Result<ExternalSnapshot, String> {
    let (object, stored_executable, generation) = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.refresh_snapshot(external_object_id)?
    };
    let observed_object = object.clone();
    let (executable, snapshot) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let executable = resolve_gh_executable(stored_executable.as_deref())
                .map_err(|error| error.to_string())?;
            let input = ExternalObjectInput {
                provider: observed_object.provider,
                kind: observed_object.kind,
                external_key: observed_object.external_key.clone(),
                canonical_url: observed_object.canonical_url.clone(),
            };
            let snapshot = GithubCli::new(executable.clone())
                .fetch(&input, current_unix_seconds())
                .map_err(|error| error.to_string())?;
            Ok((executable, snapshot))
        })
        .await
        .map_err(|error| format!("External Object refresh worker failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    runtime.apply_refresh(&object, generation, &executable, snapshot)
}

/// Read a linked GitHub Issue as a document (body and sub-issues) for display.
pub(crate) async fn fetch_issue_document_with_state(
    external_object_id: i64,
    state: &Mutex<Runtime>,
) -> Result<IssueDocument, String> {
    let (object, stored_executable) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let object = runtime
            .state
            .external_objects
            .iter()
            .find(|object| object.id == external_object_id)
            .cloned()
            .ok_or_else(|| format!("External Object {external_object_id} does not exist"))?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue {
            return Err("Only GitHub Issues can be read as a document".into());
        }
        (object, runtime.gh_executable_path.clone())
    };
    let (executable, document) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let executable = resolve_gh_executable(stored_executable.as_deref())
                .map_err(|error| error.to_string())?;
            let document = GithubCli::new(executable.clone())
                .fetch_issue_document(&object.canonical_url)
                .map_err(|error| error.to_string())?;
            Ok((executable, document))
        })
        .await
        .map_err(|error| format!("GitHub Issue reader failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    runtime.remember_gh_executable(&executable)?;
    Ok(document)
}

pub(crate) async fn add_external_comment_with_state(
    link_id: i64,
    body: String,
    state: &Mutex<Runtime>,
) -> Result<ExternalLinkView, String> {
    let body = body.trim().to_owned();
    if body.is_empty() {
        return Err("A GitHub comment cannot be blank".into());
    }
    let (link, object, stored_executable) = {
        let runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let link = runtime
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| format!("Link {link_id} does not exist"))?;
        let object = runtime
            .state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
            .cloned()
            .ok_or_else(|| "The linked External Object does not exist".to_owned())?;
        if object.provider != ExternalProvider::GitHub || object.kind == ExternalObjectKind::Generic
        {
            return Err("Comments are only supported for GitHub Issues and pull requests".into());
        }
        (link, object, runtime.gh_executable_path.clone())
    };
    let observed_object = object.clone();
    let (executable, ()) = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        let executable = resolve_gh_executable(stored_executable.as_deref())
            .map_err(|error| error.to_string())?;
        GithubCli::new(executable.clone())
            .add_comment(&observed_object.canonical_url, &body)
            .map_err(|error| error.to_string())?;
        Ok((executable, ()))
    })
    .await
    .map_err(|error| format!("GitHub comment worker failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if !runtime.state.links.iter().any(|current| current == &link)
        || !runtime
            .state
            .external_objects
            .iter()
            .any(|current| current == &object)
    {
        return Err(format!(
            "The GitHub comment was added to {}, but its Link changed while the request was running",
            object.canonical_url
        ));
    }
    runtime
        .remember_gh_executable(&executable)
        .map_err(|error| {
            format!(
                "The GitHub comment was added, but local settings could not be updated: {error}"
            )
        })?;
    external_link_view(&runtime.state, &link)
        .ok_or_else(|| "Comment target produced no External Object".to_owned())
}

struct PollEntrySnapshot {
    object: crate::domain::ExternalObject,
    generation: u64,
}

struct PollEntryObservation {
    object: crate::domain::ExternalObject,
    generation: u64,
    result: Result<crate::domain::ExternalSnapshotData, String>,
}

pub(crate) async fn poll_external_objects_with_state(
    state: &Mutex<Runtime>,
) -> Result<PollResult, String> {
    let (entries, stored_executable) = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        let mut object_ids = runtime
            .state
            .links
            .iter()
            .filter_map(|link| {
                runtime
                    .state
                    .external_objects
                    .iter()
                    .find(|object| object.id == link.external_object_id)
                    .filter(|object| object.provider == ExternalProvider::GitHub)
                    .map(|object| object.id)
            })
            .collect::<Vec<_>>();
        object_ids.sort_unstable();
        object_ids.dedup();
        let mut entries = object_ids
            .into_iter()
            .filter_map(|id| {
                runtime
                    .state
                    .external_objects
                    .iter()
                    .find(|object| object.id == id)
                    .cloned()
                    .map(|object| PollEntrySnapshot {
                        object,
                        generation: 0,
                    })
            })
            .collect::<Vec<_>>();
        for entry in &mut entries {
            entry.generation = runtime.begin_external_snapshot_request(entry.object.id);
        }
        (entries, runtime.gh_executable_path.clone())
    };
    if entries.is_empty() {
        return Ok(PollResult {
            refreshed: 0,
            failures: Vec::new(),
        });
    }
    let observations = tauri::async_runtime::spawn_blocking(move || {
        let executable = match resolve_gh_executable(stored_executable.as_deref()) {
            Ok(path) => path,
            Err(error) => {
                let error = error.to_string();
                return (
                    None,
                    entries
                        .into_iter()
                        .map(|entry| PollEntryObservation {
                            object: entry.object,
                            generation: entry.generation,
                            result: Err(error.clone()),
                        })
                        .collect::<Vec<_>>(),
                );
            }
        };
        let github = GithubCli::new(executable.clone());
        let observations = entries
            .into_iter()
            .map(|entry| {
                let input = ExternalObjectInput {
                    provider: entry.object.provider,
                    kind: entry.object.kind,
                    external_key: entry.object.external_key.clone(),
                    canonical_url: entry.object.canonical_url.clone(),
                };
                PollEntryObservation {
                    object: entry.object,
                    generation: entry.generation,
                    result: github
                        .fetch(&input, current_unix_seconds())
                        .map_err(|error| error.to_string()),
                }
            })
            .collect();
        (Some(executable), observations)
    })
    .await
    .map_err(|error| format!("External Object polling worker failed: {error}"))?;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    let mut result = PollResult {
        refreshed: 0,
        failures: Vec::new(),
    };
    if let Some(executable) = observations.0.as_deref() {
        if let Err(error) = runtime.remember_gh_executable(executable) {
            for observation in observations.1 {
                result.failures.push(PollFailure {
                    external_object_id: observation.object.id,
                    error: error.clone(),
                });
            }
            return Ok(result);
        }
    }
    for observation in observations.1 {
        let external_object_id = observation.object.id;
        match runtime.apply_poll_observation(observation) {
            Ok(()) => result.refreshed += 1,
            Err(error) => result.failures.push(PollFailure {
                external_object_id,
                error,
            }),
        }
    }
    Ok(result)
}

impl Runtime {
    #[cfg(test)]
    pub(crate) fn create_github_issue(
        &mut self,
        item_id: i64,
        repository_id: i64,
        title: String,
        body: String,
    ) -> Result<ExternalLinkAction, String> {
        const UNAVAILABLE_SCOPE: &str =
            "The Item or Repository is not available in the Item's Project";

        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == item.project_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        if !self
            .state
            .contexts
            .iter()
            .any(|context| context.id == project.context_id)
        {
            return Err(UNAVAILABLE_SCOPE.into());
        }
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        let repository_project = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == repository.project_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        if repository.project_id != project.id
            || repository_project.context_id != project.context_id
        {
            return Err(UNAVAILABLE_SCOPE.into());
        }
        let repository_name = github_repository_name(&repository.remote_url).ok_or_else(|| {
            "The selected Repository does not have a valid GitHub remote".to_owned()
        })?;
        if title.trim().is_empty() {
            return Err("A GitHub Issue title is required".into());
        }

        let executable = self.gh_executable_path()?;
        let created_url = GithubCli::new(executable.clone())
            .create_issue(&repository_name, title.trim(), &body)
            .map_err(|error| error.to_string())?;
        let object = classify_url(&created_url).map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue {
            return Err(format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: GitHub CLI returned a URL that is not a GitHub Issue"
            ));
        }

        let mut warning = None;
        let snapshot = match GithubCli::new(executable).fetch(&object, current_unix_seconds()) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                warning = Some(error.to_string());
                None
            }
        };
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id,
                object,
                snapshot,
            },
        )
        .map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| {
                format!(
                    "GitHub Issue was created at {created_url}, but linking it to the Item failed: Issue creation produced no Link"
                )
            })?;
        self.commit(decision).map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| {
                format!(
                    "GitHub Issue was created at {created_url}, but linking it to the Item failed: Issue creation produced no External Object"
                )
            })?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    #[cfg(test)]
    pub(crate) fn gh_executable_path(&mut self) -> Result<PathBuf, String> {
        let stored_path = self.gh_executable_path.as_deref();
        let executable = resolve_gh_executable(stored_path).map_err(|error| error.to_string())?;
        if self.gh_executable_path.as_deref() != Some(executable.as_path()) {
            self.store
                .set_gh_executable_path(&executable)
                .map_err(|error| error.to_string())?;
            self.gh_executable_path = Some(executable.clone());
        }
        Ok(executable)
    }
}
