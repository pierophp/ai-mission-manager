use super::*;

fn markdown_title(markdown: &str, path: &Path) -> String {
    markdown
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Local Markdown".into())
}

fn markdown_status(markdown: &str) -> String {
    markdown
        .lines()
        .take(12)
        .find_map(|line| {
            let (key, value) = line.trim().split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("status")
                .then(|| value.trim().to_owned())
        })
        .filter(|status| !status.is_empty())
        .unwrap_or_else(|| "Open".into())
}

fn looks_like_local_markdown_path(value: &str) -> bool {
    let value = value.trim();
    if value.starts_with("https://") || value.starts_with("http://") {
        return false;
    }
    let path = value.strip_prefix("file://").unwrap_or(value);
    matches!(
        Path::new(path).extension().and_then(|extension| extension.to_str()),
        Some(extension) if matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
    )
}

fn markdown_body_and_comments(markdown: &str) -> (String, String) {
    let mut body = Vec::new();
    let mut comments = Vec::new();
    let mut in_comments = false;
    for line in markdown.lines() {
        if in_comments && line.trim_start().starts_with("## ") {
            break;
        }
        if line.trim().eq_ignore_ascii_case("## Comments") {
            in_comments = true;
            continue;
        }
        if in_comments {
            comments.push(line);
        } else {
            body.push(line);
        }
    }
    (
        body.join("\n").trim().to_owned(),
        comments.join("\n").trim().to_owned(),
    )
}

pub(super) fn read_local_markdown_snapshot(
    path: &Path,
) -> Result<crate::domain::ExternalSnapshotData, String> {
    let markdown = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "Local Markdown file '{}' could not be read: {error}",
            path.display()
        )
    })?;
    let status = markdown_status(&markdown);
    Ok(crate::domain::ExternalSnapshotData {
        title: markdown_title(&markdown, path),
        state: status.clone(),
        metadata: vec![crate::domain::ExternalMetadata {
            key: "status".into(),
            value: status,
        }],
        fetched_at: current_unix_seconds(),
    })
}

fn read_local_markdown_document(path: &Path, external_key: &str) -> Result<IssueDocument, String> {
    let markdown = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "Local Markdown file '{}' is missing or unreadable: {error}",
            path.display()
        )
    })?;
    let (body, _) = markdown_body_and_comments(&markdown);
    let sub_issues = if path.file_name().and_then(|name| name.to_str()) == Some("spec.md") {
        let issues_dir = path
            .parent()
            .map(|directory| directory.join("issues"))
            .ok_or_else(|| "The local Markdown Spec has no feature directory".to_owned())?;
        let repository_id = external_key
            .strip_prefix("local:")
            .and_then(|reference| reference.split_once('#'))
            .map(|(id, _)| id)
            .ok_or_else(|| "The local Markdown Spec has an invalid repository path".to_owned())?;
        let mut ticket_paths = if issues_dir.exists() {
            std::fs::read_dir(&issues_dir)
                .map_err(|error| {
                    format!(
                        "Local Markdown ticket directory '{}' could not be read: {error}",
                        issues_dir.display()
                    )
                })?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|ticket| {
                    ticket.is_file()
                        && ticket
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| {
                                matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
                            })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        ticket_paths.sort();
        ticket_paths
            .into_iter()
            .filter_map(|ticket| {
                let ticket_markdown = std::fs::read_to_string(&ticket).ok()?;
                let title = markdown_title(&ticket_markdown, &ticket);
                let status = markdown_status(&ticket_markdown);
                let spec_relative = external_key
                    .strip_prefix("local:")
                    .and_then(|reference| reference.split_once('#'))
                    .map(|(_, relative)| Path::new(relative))?;
                let relative = spec_relative
                    .parent()?
                    .join("issues")
                    .join(ticket.file_name()?)
                    .components()
                    .filter_map(|component| component.as_os_str().to_str())
                    .collect::<Vec<_>>()
                    .join("/");
                let number = ticket
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.split('-').next())
                    .and_then(|number| number.parse().ok())
                    .unwrap_or(0);
                Some(crate::provider::SubIssue {
                    number,
                    title,
                    state: status,
                    url: format!("local:{repository_id}#{relative}"),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(IssueDocument {
        body,
        body_format: "markdown".into(),
        sub_issues,
    })
}

fn read_local_markdown_comments(path: &Path) -> Result<Vec<ExternalComment>, String> {
    let markdown = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "Local Markdown file '{}' is missing or unreadable: {error}",
            path.display()
        )
    })?;
    let (_, comments) = markdown_body_and_comments(&markdown);
    if comments.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![ExternalComment {
        id: 0,
        author: "Local Markdown".into(),
        body: comments,
        created_at: String::new(),
    }])
}

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
    pub(super) fn local_markdown_path_for_object(
        &self,
        context_id: i64,
        external_key: &str,
    ) -> Result<PathBuf, String> {
        let (repository_id, relative_path) = external_key
            .strip_prefix("local:")
            .and_then(|reference| reference.split_once('#'))
            .and_then(|(id, path)| id.parse::<i64>().ok().map(|id| (id, path)))
            .ok_or_else(|| "This local Markdown link has an invalid repository path".to_owned())?;
        let context = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        let machine = context
            .execution_machine_id
            .and_then(|id| self.state.machines.iter().find(|machine| machine.id == id))
            .filter(|machine| self.machine_access.is_local(machine))
            .ok_or_else(|| {
                format!(
                    "Local Markdown tracker in Context '{}' requires a local execution Machine because files are read from the Repository's main checkout",
                    context.name
                )
            })?;
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .filter(|repository| {
                self.state.projects.iter().any(|project| {
                    project.id == repository.project_id && project.context_id == context_id
                })
            })
            .ok_or_else(|| {
                "The Repository for this local Markdown link is unavailable".to_owned()
            })?;
        let location = self
            .state
            .repository_locations
            .iter()
            .find(|location| {
                location.repository_id == repository.id && location.machine_id == machine.id
            })
            .ok_or_else(|| {
                format!(
                    "Repository '{}' has no main checkout registered on local Machine '{}'",
                    repository.name, machine.name
                )
            })?;
        let root = Path::new(&location.checkout_path)
            .canonicalize()
            .map_err(|error| {
                format!(
                    "Could not read Repository '{}' main checkout at {}: {error}",
                    repository.name, location.checkout_path
                )
            })?;
        let relative = Path::new(relative_path);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
            || !relative
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
                })
        {
            return Err("This local Markdown link has an invalid repository path".into());
        }
        let path = root.join(relative).canonicalize().map_err(|error| {
            format!(
                "Local Markdown file '{}' is missing or unreadable in Repository '{}' main checkout: {error}",
                relative_path, repository.name
            )
        })?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err(
                "The local Markdown file is outside its registered Repository checkout".into(),
            );
        }
        Ok(path)
    }

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
        let context_id = project.context_id;
        Ok((
            IssueCreationScope {
                item_id,
                project_id: item.project_id,
                context_id,
                repository_id,
                repository_project_id: repository.project_id,
                remote_url: repository.remote_url.clone(),
            },
            self.configured_gh_path(context_id)?,
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
        self.remember_context_gh_executable(observation.scope.context_id, &observation.executable)
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
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::{create_github_issue_with_state, PollEntryObservation};
    use crate::{agent_state::AgentStateRecord, app::Runtime};

    #[test]
    fn context_identifiers_warn_only_when_the_url_targets_a_different_provider_account() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        let context = &mut runtime.state.contexts[0];
        context.atlassian_site = Some("acme.atlassian.net".into());
        context.bitbucket_workspace = Some("acme-code".into());
        context.azure_devops_organization = Some("acme-engineering".into());

        let atlassian_warning = runtime
            .context_provider_mismatch_warning(1, "https://other.atlassian.net/browse/PROJ-1");
        assert!(atlassian_warning.is_some());

        let bitbucket_warning = runtime.context_provider_mismatch_warning(
            1,
            "https://bitbucket.org/another-team/app/pull-requests/4",
        );
        assert!(bitbucket_warning.is_some());

        let azure_warning = runtime.context_provider_mismatch_warning(
            1,
            "https://dev.azure.com/another-org/project/_workitems/edit/17",
        );
        assert!(azure_warning.is_some());

        assert!(runtime
            .context_provider_mismatch_warning(1, "https://github.com/acme/app/issues/4")
            .is_none());
    }

    #[test]
    fn github_executable_resolution_is_scoped_to_each_context_without_global_fallback() {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        let second_context = runtime
            .create_context("Second Context".into())
            .expect("second Context should be created");
        runtime.state.contexts[0].gh_executable_path = Some("/tools/first/gh".into());
        assert_eq!(
            runtime
                .configured_gh_path(1)
                .expect("first path should resolve"),
            Some(PathBuf::from("/tools/first/gh"))
        );
        assert_eq!(
            runtime
                .configured_gh_path(second_context.id)
                .expect("second path should resolve"),
            None,
            "an unset new Context must use PATH resolution, not an app-wide stored path"
        );
        runtime
            .state
            .contexts
            .iter_mut()
            .find(|context| context.id == second_context.id)
            .expect("second Context should exist")
            .gh_executable_path = Some("/tools/second/gh".into());
        assert_eq!(
            runtime
                .configured_gh_path(second_context.id)
                .expect("second path should resolve"),
            Some(PathBuf::from("/tools/second/gh"))
        );
    }

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
            .create_item("Issue target".into(), 1, 1, String::new())
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
            cli_configuration_profile: None,
            execution_profile: crate::domain::ExecutionProfile::Implement,
            workflow: crate::domain::Workflow::MattPocock,
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
            reported_pull_requests: Vec::new(),
            attention_summary: None,
            grill_question_group: None,
            grill_answers: Vec::new(),
            grill_decisions: Vec::new(),
            grill_response: None,
            grill_phase: None,
            grill_action: None,
            grill_action_started_at: None,
            plan_phase: None,
            plan_path: None,
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
            .set_context_gh_executable_path(1, &executable)
            .expect("fake gh path should persist");
        runtime.state.contexts[0].gh_executable_path =
            Some(executable.to_string_lossy().into_owned());
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
            purpose: crate::domain::LinkPurpose::Others,
            spec_external_object_id: None,
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
                context_id: 1,
                executable: None,
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

    #[test]
    fn polling_uses_each_context_provider_configuration_and_skips_local_objects() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            sync::{Arc, Mutex},
        };

        use crate::domain::{
            Event, ExternalObjectInput, ExternalObjectKind, ExternalProvider, ProjectDefaults,
        };

        let directory = tempdir().expect("temporary app directory should exist");
        let first_cli = directory.path().join("twg-first");
        let second_cli = directory.path().join("az-second");
        let third_cli = directory.path().join("twg-third");
        let fourth_cli = directory.path().join("twg-failing");
        let first_log = directory.path().join("first.log");
        let second_log = directory.path().join("second.log");
        let third_log = directory.path().join("third.log");
        let scripts = [
            (
                &first_cli,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf '%s' '{{\"title\":\"Jira title\",\"status\":\"In Progress\"}}'\n",
                    first_log.display()
                ),
            ),
            (
                &second_cli,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf '%s' '{{\"fields\":{{\"System.Title\":\"ADO title\",\"System.State\":\"Active\"}}}}'\n",
                    second_log.display()
                ),
            ),
            (
                &third_cli,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf '%s' '{{\"title\":\"Third Jira title\",\"status\":\"Open\"}}'\n",
                    third_log.display()
                ),
            ),
            (
                &fourth_cli,
                "#!/bin/sh\nprintf '%s' 'isolated Context failure' >&2\nexit 1\n".into(),
            ),
        ];
        for (executable, script) in scripts {
            fs::write(executable, script).expect("fake provider CLI should be written");
            fs::set_permissions(executable, fs::Permissions::from_mode(0o755))
                .expect("fake provider CLI should be executable");
        }

        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("Runtime should open");
        let second_context = runtime
            .create_context("Second Context".into())
            .expect("second Context should be created");
        let second_project = runtime
            .create_project(
                "Second Project".into(),
                second_context.id,
                ProjectDefaults::default(),
            )
            .expect("second Project should be created");
        let second_item = runtime
            .create_item(
                "ADO work item".into(),
                second_context.id,
                second_project.id,
                String::new(),
            )
            .expect("second Item should be created");
        let third_context = runtime
            .create_context("Third Context".into())
            .expect("third Context should be created");
        let third_project = runtime
            .create_project(
                "Third Project".into(),
                third_context.id,
                ProjectDefaults::default(),
            )
            .expect("third Project should be created");
        let third_item = runtime
            .create_item(
                "Failing Jira work item".into(),
                third_context.id,
                third_project.id,
                String::new(),
            )
            .expect("third Item should be created");
        let fourth_context = runtime
            .create_context("Fourth Context".into())
            .expect("fourth Context should be created");
        let fourth_project = runtime
            .create_project(
                "Fourth Project".into(),
                fourth_context.id,
                ProjectDefaults::default(),
            )
            .expect("fourth Project should be created");
        let fourth_item = runtime
            .create_item(
                "Failing Jira work item".into(),
                fourth_context.id,
                fourth_project.id,
                String::new(),
            )
            .expect("fourth Item should be created");
        let first_item = runtime
            .create_item("First Jira work item".into(), 1, 1, String::new())
            .expect("first Item should be created");
        {
            let first_context = runtime
                .state
                .contexts
                .iter_mut()
                .find(|context| context.id == 1)
                .expect("first Context should exist");
            first_context.twg_executable_path = Some(first_cli.to_string_lossy().into_owned());
            first_context.atlassian_site = Some("first.atlassian.net".into());
        }
        {
            let context = runtime
                .state
                .contexts
                .iter_mut()
                .find(|context| context.id == second_context.id)
                .expect("second Context should exist");
            context.az_executable_path = Some(second_cli.to_string_lossy().into_owned());
            context.azure_devops_organization = Some("https://dev.azure.com/second-org".into());
        }
        {
            let context = runtime
                .state
                .contexts
                .iter_mut()
                .find(|context| context.id == third_context.id)
                .expect("third Context should exist");
            context.twg_executable_path = Some(third_cli.to_string_lossy().into_owned());
            context.atlassian_site = Some("third.atlassian.net".into());
        }
        {
            let context = runtime
                .state
                .contexts
                .iter_mut()
                .find(|context| context.id == fourth_context.id)
                .expect("fourth Context should exist");
            context.twg_executable_path = Some(fourth_cli.to_string_lossy().into_owned());
            context.atlassian_site = Some("fourth.atlassian.net".into());
        }
        for (item_id, provider, kind, external_key, canonical_url) in [
            (
                first_item.id,
                ExternalProvider::Atlassian,
                ExternalObjectKind::Issue,
                "jira:PROJ#PROJ-7",
                "https://first.atlassian.net/browse/PROJ-7",
            ),
            (
                second_item.id,
                ExternalProvider::AzureDevOps,
                ExternalObjectKind::Issue,
                "ado:second-org/project#12",
                "https://dev.azure.com/second-org/project/_workitems/edit/12",
            ),
            (
                third_item.id,
                ExternalProvider::Atlassian,
                ExternalObjectKind::Issue,
                "jira:OTHER#OTHER-4",
                "https://third.atlassian.net/browse/OTHER-4",
            ),
            (
                fourth_item.id,
                ExternalProvider::Atlassian,
                ExternalObjectKind::Issue,
                "jira:FAIL#FAIL-5",
                "https://fourth.atlassian.net/browse/FAIL-5",
            ),
            (
                first_item.id,
                ExternalProvider::Generic,
                ExternalObjectKind::Generic,
                "local:9#.scratch/spec.md",
                "file:///repository/.scratch/spec.md",
            ),
        ] {
            let decision = crate::domain::decide(
                runtime.state.clone(),
                Event::LinkExternalObject {
                    item_id,
                    object: ExternalObjectInput {
                        provider,
                        kind,
                        external_key: external_key.into(),
                        canonical_url: canonical_url.into(),
                    },
                    snapshot: None,
                },
            )
            .expect("test External Object should link");
            runtime
                .commit(decision)
                .expect("External Object should persist");
        }

        let state = Arc::new(Mutex::new(runtime));
        let result =
            tauri::async_runtime::block_on(super::poll_external_objects_with_state(state.as_ref()))
                .expect("poll should report individual provider failures");

        assert_eq!(result.refreshed, 3, "poll failures: {:?}", result.failures);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].external_object_id, 4);
        assert!(result.failures[0]
            .error
            .contains("isolated Context failure"));
        assert_eq!(
            fs::read_to_string(&first_log)
                .expect("first Context CLI should run")
                .trim(),
            "jira workitem get PROJ-7 --site first.atlassian.net --output json"
        );
        assert!(fs::read_to_string(&second_log)
            .expect("second Context CLI should run")
            .contains("--organization https://dev.azure.com/second-org -o json"));
        assert_eq!(
            fs::read_to_string(&third_log)
                .expect("third Context CLI should run")
                .trim(),
            "jira workitem get OTHER-4 --site third.atlassian.net --output json"
        );
        let runtime = state.lock().expect("Runtime should remain available");
        assert_eq!(runtime.state.snapshots.len(), 3);
        assert!(runtime
            .state
            .snapshots
            .iter()
            .any(|snapshot| snapshot.title == "Jira title"));
        assert!(runtime
            .state
            .snapshots
            .iter()
            .any(|snapshot| snapshot.title == "ADO title"));
        assert!(runtime
            .state
            .snapshots
            .iter()
            .any(|snapshot| snapshot.title == "Third Jira title"));
        let local_object_id = runtime
            .state
            .external_objects
            .iter()
            .find(|object| object.provider == ExternalProvider::Generic)
            .expect("local Generic External Object should exist")
            .id;
        assert!(
            !runtime
                .external_snapshot_request_generations
                .contains_key(&local_object_id),
            "a local Generic External Object must never enter the polling requests"
        );
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
        let github = ProviderDispatch::new(ExternalProvider::GitHub, Some(executable.clone()));
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
        let (snapshot, warning) = match github.fetch_snapshot(&object, current_unix_seconds()) {
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
    context_id: i64,
    input: ExternalObjectInput,
    existing: Option<crate::domain::ExternalObject>,
    stored_executable: Option<PathBuf>,
    site: Option<String>,
    workspace: Option<String>,
    organization: Option<String>,
    local_markdown_path: Option<PathBuf>,
}

struct LinkObservation {
    snapshot: LinkSnapshot,
    executable: Option<PathBuf>,
    fetched: Option<crate::domain::ExternalSnapshotData>,
    warning: Option<String>,
}

impl Runtime {
    fn configured_gh_path(&self, context_id: i64) -> Result<Option<PathBuf>, String> {
        let context = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        Ok(context.gh_executable_path.as_deref().map(PathBuf::from))
    }

    fn configured_twg(
        &self,
        context_id: i64,
    ) -> Result<(Option<PathBuf>, Option<String>, Option<String>), String> {
        let context = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        Ok((
            context.twg_executable_path.as_deref().map(PathBuf::from),
            context.atlassian_site.clone(),
            context.bitbucket_workspace.clone(),
        ))
    }

    pub(super) fn configured_provider(
        &self,
        context_id: i64,
        provider: ExternalProvider,
    ) -> Result<
        (
            Option<PathBuf>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
        String,
    > {
        match provider {
            ExternalProvider::Atlassian => {
                let (path, site, workspace) = self.configured_twg(context_id)?;
                Ok((path, site, workspace, None))
            }
            ExternalProvider::AzureDevOps => {
                let context = self
                    .state
                    .contexts
                    .iter()
                    .find(|context| context.id == context_id)
                    .ok_or_else(|| format!("Context {context_id} does not exist"))?;
                Ok((
                    context.az_executable_path.as_deref().map(PathBuf::from),
                    None,
                    None,
                    context.azure_devops_organization.clone(),
                ))
            }
            _ => Ok((self.configured_gh_path(context_id)?, None, None, None)),
        }
    }

    fn remember_context_gh_executable(
        &mut self,
        context_id: i64,
        executable: &Path,
    ) -> Result<(), String> {
        let context = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)
            .cloned()
            .ok_or_else(|| format!("Context {context_id} does not exist"))?;
        if context.gh_executable_path.as_deref() == Some(&*executable.to_string_lossy()) {
            return Ok(());
        }
        self.store
            .set_context_gh_executable_path(context_id, executable)
            .map_err(|error| error.to_string())?;
        if let Some(context) = self
            .state
            .contexts
            .iter_mut()
            .find(|context| context.id == context_id)
        {
            context.gh_executable_path = Some(executable.to_string_lossy().into_owned());
        }
        Ok(())
    }

    fn remember_context_provider_executable(
        &mut self,
        context_id: i64,
        provider: ExternalProvider,
        executable: &Path,
    ) -> Result<(), String> {
        if provider == ExternalProvider::Atlassian {
            let context = self
                .state
                .contexts
                .iter()
                .find(|context| context.id == context_id)
                .cloned()
                .ok_or_else(|| format!("Context {context_id} does not exist"))?;
            if context.twg_executable_path.as_deref() == Some(&*executable.to_string_lossy()) {
                return Ok(());
            }
            self.store
                .set_context_twg_executable_path(context_id, executable)
                .map_err(|error| error.to_string())?;
            if let Some(context) = self
                .state
                .contexts
                .iter_mut()
                .find(|context| context.id == context_id)
            {
                context.twg_executable_path = Some(executable.to_string_lossy().into_owned());
            }
            Ok(())
        } else if provider == ExternalProvider::AzureDevOps {
            let context = self
                .state
                .contexts
                .iter()
                .find(|context| context.id == context_id)
                .cloned()
                .ok_or_else(|| format!("Context {context_id} does not exist"))?;
            if context.az_executable_path.as_deref() == Some(&*executable.to_string_lossy()) {
                return Ok(());
            }
            self.store
                .set_context_az_executable_path(context_id, executable)
                .map_err(|error| error.to_string())?;
            if let Some(context) = self
                .state
                .contexts
                .iter_mut()
                .find(|context| context.id == context_id)
            {
                context.az_executable_path = Some(executable.to_string_lossy().into_owned());
            }
            Ok(())
        } else {
            self.remember_context_gh_executable(context_id, executable)
        }
    }

    fn context_provider_mismatch_warning(
        &self,
        context_id: i64,
        canonical_url: &str,
    ) -> Option<String> {
        let context = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)?;
        let target = canonical_url.split_once("://")?.1;
        let host_and_path = target.split_once('/').unwrap_or((target, ""));
        let host = host_and_path.0.to_ascii_lowercase();
        let path = host_and_path.1;
        let site_mismatch = context.atlassian_site.as_ref().is_some_and(|configured| {
            host.ends_with(".atlassian.net")
                && host.strip_suffix(".atlassian.net").is_some_and(|site| {
                    !site.eq_ignore_ascii_case(configured.trim().trim_end_matches(".atlassian.net"))
                })
        });
        let organization_mismatch =
            context
                .azure_devops_organization
                .as_ref()
                .is_some_and(|configured| {
                    let configured = configured
                        .trim()
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or(configured);
                    host == "dev.azure.com"
                        && path.split('/').next().is_some_and(|organization| {
                            !organization.eq_ignore_ascii_case(configured)
                        })
                });
        let bitbucket_mismatch = context
            .bitbucket_workspace
            .as_ref()
            .is_some_and(|configured| {
                host == "bitbucket.org"
                    && path
                        .split('/')
                        .next()
                        .is_some_and(|workspace| !workspace.eq_ignore_ascii_case(configured))
            });
        let mismatch = site_mismatch || organization_mismatch || bitbucket_mismatch;
        mismatch.then(|| format!(
            "This External Object targets a different site or organization than the identifiers configured for Context '{}'. The Link was created.",
            context.name
        ))
    }

    fn link_snapshot(&self, item_id: i64, url: &str) -> Result<LinkSnapshot, String> {
        let context_id = self.item_context_id(item_id)?;
        let local_markdown = self.classify_local_markdown_for_item(item_id, url);
        let input = match local_markdown {
            Some(input) => input,
            None if looks_like_local_markdown_path(url) => {
                let context = self
                    .state
                    .contexts
                    .iter()
                    .find(|context| context.id == context_id)
                    .ok_or_else(|| format!("Context {context_id} does not exist"))?;
                return Err(format!(
                    "Local Markdown files must be inside a registered Repository main checkout in a Context with a local execution Machine; Context '{}' does not provide that local file access",
                    context.name
                ));
            }
            None => classify_url(url).map_err(|error| error.to_string())?,
        };
        if input.external_key.starts_with("local:") {
            let context = self
                .state
                .contexts
                .iter()
                .find(|context| context.id == context_id)
                .ok_or_else(|| format!("Context {context_id} does not exist"))?;
            if !context.execution_machine_id.is_some_and(|machine_id| {
                self.state.machines.iter().any(|machine| {
                    machine.id == machine_id && self.machine_access.is_local(machine)
                })
            }) {
                return Err(format!(
                    "Local Markdown tracker in Context '{}' requires a local execution Machine because files are read from the Repository's main checkout",
                    context.name
                ));
            }
        }
        let local_markdown_path = input
            .external_key
            .starts_with("local:")
            .then(|| self.local_markdown_path_for_object(context_id, &input.external_key))
            .transpose()?;
        let existing = self
            .state
            .external_objects
            .iter()
            .find(|object| {
                object.provider == input.provider && object.external_key == input.external_key
            })
            .cloned();
        let (stored_executable, site, workspace, organization) = if local_markdown_path.is_some() {
            (None, None, None, None)
        } else {
            self.configured_provider(context_id, input.provider)?
        };
        Ok(LinkSnapshot {
            item_id,
            context_id,
            input,
            existing,
            stored_executable,
            site,
            workspace,
            organization,
            local_markdown_path,
        })
    }

    fn classify_local_markdown_for_item(
        &self,
        item_id: i64,
        path: &str,
    ) -> Option<ExternalObjectInput> {
        let item = self.state.items.iter().find(|item| item.id == item_id)?;
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == item.project_id)?;
        let context_id = self.item_context_id(item_id).ok()?;
        let execution_machine_id = self
            .state
            .contexts
            .iter()
            .find(|context| context.id == context_id)?
            .execution_machine_id?;
        let execution_machine = self
            .state
            .machines
            .iter()
            .find(|machine| machine.id == execution_machine_id)?;
        if !self.machine_access.is_local(execution_machine) {
            return None;
        }
        for repository in self
            .state
            .repositories
            .iter()
            .filter(|repository| repository.project_id == project.id)
        {
            for location in self
                .state
                .repository_locations
                .iter()
                .filter(|location| location.repository_id == repository.id)
            {
                if location.machine_id != execution_machine_id {
                    continue;
                }
                if let Some(object) =
                    classify_local_markdown(repository.id, Path::new(&location.checkout_path), path)
                {
                    return Some(object);
                }
            }
        }
        None
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
            if let Err(error) = self.remember_context_provider_executable(
                observation.snapshot.context_id,
                observation.snapshot.input.provider,
                &executable,
            ) {
                warning.get_or_insert(error);
            }
        }
        if let Some(mismatch_warning) = self.context_provider_mismatch_warning(
            observation.snapshot.context_id,
            &observation.snapshot.input.canonical_url,
        ) {
            warning = Some(match warning {
                Some(existing) => format!("{existing}; {mismatch_warning}"),
                None => mismatch_warning,
            });
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
    ) -> Result<
        (
            crate::domain::ExternalObject,
            Option<PathBuf>,
            Option<String>,
            Option<String>,
            Option<String>,
            u64,
        ),
        String,
    > {
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == external_object_id)
            .cloned()
            .ok_or_else(|| format!("External Object {external_object_id} does not exist"))?;
        let generation = self.begin_external_snapshot_request(external_object_id);
        let context_id = self
            .state
            .links
            .iter()
            .find(|link| link.external_object_id == external_object_id)
            .map(|link| self.item_context_id(link.item_id))
            .transpose()?
            .ok_or_else(|| format!("External Object {external_object_id} has no Link"))?;
        let (executable, site, workspace, organization) =
            self.configured_provider(context_id, object.provider)?;
        Ok((
            object,
            executable,
            site,
            workspace,
            organization,
            generation,
        ))
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
        let context_id = self
            .state
            .links
            .iter()
            .find(|link| link.external_object_id == object.id)
            .map(|link| self.item_context_id(link.item_id))
            .transpose()?
            .ok_or_else(|| format!("External Object {} has no Link", object.id))?;
        self.remember_context_provider_executable(context_id, object.provider, executable)?;
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
        if snapshot.existing.is_none() {
            if let Some(path) = snapshot.local_markdown_path.as_deref() {
                match read_local_markdown_snapshot(path) {
                    Ok(value) => fetched = Some(value),
                    Err(error) => warning = Some(error),
                }
            } else {
                let provider = ProviderDispatch::new(
                    snapshot.input.provider,
                    snapshot.stored_executable.clone(),
                )
                .with_site(snapshot.site.clone())
                .with_workspace(snapshot.workspace.clone())
                .with_organization(snapshot.organization.clone());
                match provider.resolved_executable() {
                    Ok(path) => {
                        match provider.fetch_snapshot(&snapshot.input, current_unix_seconds()) {
                            Ok(value) => fetched = Some(value),
                            Err(error) => warning = Some(error.to_string()),
                        }
                        executable = Some(path);
                    }
                    Err(error) => warning = Some(error.to_string()),
                }
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
    let (object, stored_executable, site, workspace, organization, generation) = {
        let mut runtime = state
            .lock()
            .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
        runtime.refresh_snapshot(external_object_id)?
    };
    let observed_object = object.clone();
    let (executable, snapshot) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let input = ExternalObjectInput {
                provider: observed_object.provider,
                kind: observed_object.kind,
                external_key: observed_object.external_key.clone(),
                canonical_url: observed_object.canonical_url.clone(),
            };
            let provider = ProviderDispatch::new(observed_object.provider, stored_executable)
                .with_site(site)
                .with_workspace(workspace)
                .with_organization(organization);
            let executable = provider
                .resolved_executable()
                .map_err(|error| error.to_string())?;
            let snapshot = provider
                .fetch_snapshot(&input, current_unix_seconds())
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
    let (object, context_id, stored_executable, site, workspace, organization, local_path) = {
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
        if !object.external_key.starts_with("local:")
            && !matches!(
                object.kind,
                ExternalObjectKind::Issue | ExternalObjectKind::Document
            )
        {
            return Err("Only Issues and documents can be read as a spec document".into());
        }
        let context_id = runtime
            .state
            .links
            .iter()
            .find(|link| link.external_object_id == external_object_id)
            .map(|link| runtime.item_context_id(link.item_id))
            .transpose()?
            .ok_or_else(|| format!("External Object {external_object_id} has no Link"))?;
        let local_path = object
            .external_key
            .starts_with("local:")
            .then(|| runtime.local_markdown_path_for_object(context_id, &object.external_key))
            .transpose()?;
        let (path, site, workspace, organization) = if local_path.is_some() {
            (None, None, None, None)
        } else {
            runtime.configured_provider(context_id, object.provider)?
        };
        (
            object,
            context_id,
            path,
            site,
            workspace,
            organization,
            local_path,
        )
    };
    let provider_kind = object.provider;
    let (executable, document) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            if let Some(path) = local_path.as_deref() {
                let document = read_local_markdown_document(path, &object.external_key)?;
                return Ok((None, document));
            }
            let provider = ProviderDispatch::new(object.provider, stored_executable)
                .with_site(site)
                .with_workspace(workspace)
                .with_organization(organization);
            let executable = provider
                .resolved_executable()
                .map_err(|error| error.to_string())?;
            let input = ExternalObjectInput {
                provider: object.provider,
                kind: object.kind,
                external_key: object.external_key.clone(),
                canonical_url: object.canonical_url.clone(),
            };
            let content = provider
                .fetch_document_with_format(&input)
                .map_err(|error| error.to_string())?;
            let sub_issues = if object.provider == ExternalProvider::GitHub {
                provider
                    .list_tickets(&input)
                    .map_err(|error| error.to_string())?
            } else {
                Vec::new()
            };
            let document = IssueDocument {
                body: content.body,
                body_format: content.body_format,
                sub_issues,
            };
            Ok((Some(executable), document))
        })
        .await
        .map_err(|error| format!("External document reader failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if let Some(executable) = executable {
        runtime.remember_context_provider_executable(context_id, provider_kind, &executable)?;
    }
    Ok(document)
}

pub(crate) async fn fetch_external_document_with_state(
    external_object_id: i64,
    state: &Mutex<Runtime>,
) -> Result<String, String> {
    let (object, context_id, stored_executable, site, workspace, organization, local_path) = {
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
        let context_id = runtime
            .state
            .links
            .iter()
            .find(|link| link.external_object_id == external_object_id)
            .map(|link| runtime.item_context_id(link.item_id))
            .transpose()?
            .ok_or_else(|| format!("External Object {external_object_id} has no Link"))?;
        let local_path = object
            .external_key
            .starts_with("local:")
            .then(|| runtime.local_markdown_path_for_object(context_id, &object.external_key))
            .transpose()?;
        let (path, site, workspace, organization) = if local_path.is_some() {
            (None, None, None, None)
        } else {
            runtime.configured_provider(context_id, object.provider)?
        };
        (
            object,
            context_id,
            path,
            site,
            workspace,
            organization,
            local_path,
        )
    };
    let provider_kind = object.provider;
    let (executable, body) = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        if let Some(path) = local_path.as_deref() {
            let document = read_local_markdown_document(path, &object.external_key)?;
            return Ok((None, document.body));
        }
        let input = ExternalObjectInput {
            provider: object.provider,
            kind: object.kind,
            external_key: object.external_key.clone(),
            canonical_url: object.canonical_url.clone(),
        };
        let provider = ProviderDispatch::new(object.provider, stored_executable)
            .with_site(site)
            .with_workspace(workspace)
            .with_organization(organization);
        let executable = provider
            .resolved_executable()
            .map_err(|error| error.to_string())?;
        let body = provider
            .fetch_document(&input)
            .map_err(|error| error.to_string())?;
        Ok((Some(executable), body))
    })
    .await
    .map_err(|error| format!("External document reader failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if let Some(executable) = executable {
        runtime.remember_context_provider_executable(context_id, provider_kind, &executable)?;
    }
    Ok(body)
}

pub(crate) async fn fetch_external_comments_with_state(
    external_object_id: i64,
    state: &Mutex<Runtime>,
) -> Result<Vec<ExternalComment>, String> {
    let (object, context_id, stored_executable, site, workspace, organization, local_path) = {
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
        let context_id = runtime
            .state
            .links
            .iter()
            .find(|link| link.external_object_id == external_object_id)
            .map(|link| runtime.item_context_id(link.item_id))
            .transpose()?
            .ok_or_else(|| format!("External Object {external_object_id} has no Link"))?;
        let local_path = object
            .external_key
            .starts_with("local:")
            .then(|| runtime.local_markdown_path_for_object(context_id, &object.external_key))
            .transpose()?;
        let (path, site, workspace, organization) = if local_path.is_some() {
            (None, None, None, None)
        } else {
            runtime.configured_provider(context_id, object.provider)?
        };
        (
            object,
            context_id,
            path,
            site,
            workspace,
            organization,
            local_path,
        )
    };
    let provider_kind = object.provider;
    let (executable, comments) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            if let Some(path) = local_path.as_deref() {
                return Ok((None, read_local_markdown_comments(path)?));
            }
            let input = ExternalObjectInput {
                provider: object.provider,
                kind: object.kind,
                external_key: object.external_key.clone(),
                canonical_url: object.canonical_url.clone(),
            };
            let dispatch = ProviderDispatch::new(object.provider, stored_executable)
                .with_site(site)
                .with_workspace(workspace)
                .with_organization(organization);
            let executable = dispatch
                .resolved_executable()
                .map_err(|error| error.to_string())?;
            let comments = dispatch
                .fetch_comments(&input)
                .map_err(|error| error.to_string())?;
            Ok((Some(executable), comments))
        })
        .await
        .map_err(|error| format!("External comment reader failed: {error}"))??;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    if let Some(executable) = executable {
        runtime.remember_context_provider_executable(context_id, provider_kind, &executable)?;
    }
    Ok(comments)
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
    let (link, object, context_id, stored_executable) = {
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
        if object.kind == ExternalObjectKind::Generic {
            return Err("Comments are only supported for provider Issues and pull requests".into());
        }
        let context_id = runtime.item_context_id(link.item_id)?;
        (
            link,
            object,
            context_id,
            runtime.configured_gh_path(context_id)?,
        )
    };
    let observed_object = object.clone();
    let (executable, ()) = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        let provider = ProviderDispatch::new(observed_object.provider, stored_executable);
        let executable = provider
            .resolved_executable()
            .map_err(|error| error.to_string())?;
        let input = ExternalObjectInput {
            provider: observed_object.provider,
            kind: observed_object.kind,
            external_key: observed_object.external_key.clone(),
            canonical_url: observed_object.canonical_url.clone(),
        };
        provider
            .add_comment(&input, &body)
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
        .remember_context_gh_executable(context_id, &executable)
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
    context_id: i64,
    stored_executable: Option<PathBuf>,
    site: Option<String>,
    workspace: Option<String>,
    organization: Option<String>,
}

struct PollEntryObservation {
    object: crate::domain::ExternalObject,
    generation: u64,
    context_id: i64,
    executable: Option<PathBuf>,
    result: Result<crate::domain::ExternalSnapshotData, String>,
}

pub(crate) async fn poll_external_objects_with_state(
    state: &Mutex<Runtime>,
) -> Result<PollResult, String> {
    let (groups, mut preparation_failures) = {
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
                    .filter(|object| {
                        matches!(
                            object.provider,
                            ExternalProvider::GitHub
                                | ExternalProvider::Atlassian
                                | ExternalProvider::AzureDevOps
                        )
                    })
                    .map(|object| object.id)
            })
            .collect::<Vec<_>>();
        object_ids.sort_unstable();
        object_ids.dedup();
        let mut groups = std::collections::BTreeMap::<i64, Vec<PollEntrySnapshot>>::new();
        let mut failures = Vec::new();
        for id in object_ids {
            let Some(object) = runtime
                .state
                .external_objects
                .iter()
                .find(|object| object.id == id)
                .cloned()
            else {
                failures.push(PollFailure {
                    external_object_id: id,
                    error: format!("External Object {id} does not exist"),
                });
                continue;
            };
            let Some(link) = runtime
                .state
                .links
                .iter()
                .find(|link| link.external_object_id == id)
            else {
                failures.push(PollFailure {
                    external_object_id: id,
                    error: format!("External Object {id} has no Link"),
                });
                continue;
            };
            let context_id = match runtime.item_context_id(link.item_id) {
                Ok(context_id) => context_id,
                Err(error) => {
                    failures.push(PollFailure {
                        external_object_id: id,
                        error,
                    });
                    continue;
                }
            };
            let (stored_executable, site, workspace, organization) =
                match runtime.configured_provider(context_id, object.provider) {
                    Ok(configuration) => configuration,
                    Err(error) => {
                        failures.push(PollFailure {
                            external_object_id: id,
                            error,
                        });
                        continue;
                    }
                };
            let generation = runtime.begin_external_snapshot_request(object.id);
            groups
                .entry(context_id)
                .or_default()
                .push(PollEntrySnapshot {
                    object,
                    generation,
                    context_id,
                    stored_executable,
                    site,
                    workspace,
                    organization,
                });
        }
        (groups, failures)
    };
    if groups.is_empty() {
        return Ok(PollResult {
            refreshed: 0,
            failures: preparation_failures,
        });
    }
    let observations = tauri::async_runtime::spawn_blocking(move || {
        groups
            .into_values()
            .flatten()
            .map(|entry| {
                let dispatch =
                    ProviderDispatch::new(entry.object.provider, entry.stored_executable.clone())
                        .with_site(entry.site.clone())
                        .with_workspace(entry.workspace.clone())
                        .with_organization(entry.organization.clone());
                match dispatch.resolved_executable() {
                    Ok(executable) => {
                        let input = ExternalObjectInput {
                            provider: entry.object.provider,
                            kind: entry.object.kind,
                            external_key: entry.object.external_key.clone(),
                            canonical_url: entry.object.canonical_url.clone(),
                        };
                        let result =
                            ProviderDispatch::new(entry.object.provider, Some(executable.clone()))
                                .with_site(entry.site)
                                .with_workspace(entry.workspace)
                                .with_organization(entry.organization)
                                .fetch_snapshot(&input, current_unix_seconds())
                                .map_err(|error| error.to_string());
                        PollEntryObservation {
                            object: entry.object,
                            generation: entry.generation,
                            context_id: entry.context_id,
                            executable: Some(executable),
                            result,
                        }
                    }
                    Err(error) => PollEntryObservation {
                        object: entry.object,
                        generation: entry.generation,
                        context_id: entry.context_id,
                        executable: None,
                        result: Err(format!("{}", error)),
                    },
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| format!("External Object polling worker failed: {error}"))?;
    let mut runtime = state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?;
    let mut result = PollResult {
        refreshed: 0,
        failures: std::mem::take(&mut preparation_failures),
    };
    for observation in observations {
        let external_object_id = observation.object.id;
        if let Some(executable) = observation.executable.as_deref() {
            if let Err(error) = runtime.remember_context_provider_executable(
                observation.context_id,
                observation.object.provider,
                executable,
            ) {
                result.failures.push(PollFailure {
                    external_object_id,
                    error,
                });
                continue;
            }
        }
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

#[cfg(test)]
mod local_markdown_tests {
    use std::sync::Mutex;

    use tempfile::tempdir;

    use super::{
        fetch_external_comments_with_state, fetch_issue_document_with_state,
        link_external_object_with_state,
    };
    use crate::{
        app::Runtime,
        domain::{MachineTransport, RepositoryLocation},
    };

    #[test]
    fn local_queue_ticket_status_comes_from_its_status_line() {
        let directory = tempdir().expect("temporary directory should exist");
        let ticket = directory.path().join("ticket.md");
        std::fs::write(&ticket, "# Work item\nStatus: Closed\n\nDetails.\n")
            .expect("ticket file should be written");

        let snapshot = super::read_local_markdown_snapshot(&ticket)
            .expect("local ticket status should be read from Markdown");

        assert_eq!(snapshot.state, "Closed");
    }

    #[test]
    fn local_markdown_spec_reads_main_checkout_body_status_tickets_and_comments_without_worktree() {
        let directory = tempdir().expect("temporary directory should exist");
        let checkout = directory.path().join("repository");
        let issue_directory = checkout.join(".scratch/feature/issues");
        std::fs::create_dir_all(&issue_directory).expect("issue directory should be created");
        let spec_path = checkout.join(".scratch/feature/spec.md");
        std::fs::write(
            &spec_path,
            "# Feature spec\nStatus: In Progress\n\nSpec body.\n\n## Comments\n\nA local note.",
        )
        .expect("Spec should be written");
        std::fs::write(
            issue_directory.join("01-first-ticket.md"),
            "# First ticket\nStatus: Open\n\nTicket body.",
        )
        .expect("ticket should be written");

        let mut runtime = Runtime::open(&directory.path().join("mission-manager.sqlite"))
            .expect("runtime should open");
        let item = runtime
            .create_item("Local Markdown feature".into(), 1, 1, String::new())
            .expect("Item should be created");
        let machine = runtime
            .register_machine(
                1,
                "Local Machine".into(),
                "local-markdown".into(),
                MachineTransport::Local,
            )
            .expect("local Machine should be registered");
        runtime
            .set_context_execution_machine(1, Some(machine.id))
            .expect("Context should use the local Machine");
        let repository = runtime
            .register_repository(
                1,
                "repository".into(),
                "https://example.test/repository.git".into(),
            )
            .expect("Repository should be registered");
        runtime.state.repository_locations.push(RepositoryLocation {
            repository_id: repository.id,
            machine_id: machine.id,
            checkout_path: checkout.to_string_lossy().into_owned(),
            worktree_root: directory
                .path()
                .join("worktrees")
                .to_string_lossy()
                .into_owned(),
        });

        let shared_state = Mutex::new(runtime);
        let action = tauri::async_runtime::block_on(link_external_object_with_state(
            item.id,
            spec_path.to_string_lossy().into_owned(),
            &shared_state,
        ))
        .expect("local Spec should link");
        assert!(action.warning.is_none());
        let document = tauri::async_runtime::block_on(fetch_issue_document_with_state(
            action.link.object.id,
            &shared_state,
        ))
        .expect("local Spec document should load without a Worktree");
        assert_eq!(
            document.body,
            "# Feature spec\nStatus: In Progress\n\nSpec body."
        );
        assert_eq!(document.sub_issues.len(), 1);
        assert_eq!(document.sub_issues[0].number, 1);
        assert_eq!(document.sub_issues[0].title, "First ticket");
        assert_eq!(document.sub_issues[0].state, "Open");
        assert!(document.sub_issues[0]
            .url
            .ends_with("#.scratch/feature/issues/01-first-ticket.md"));
        let comments = tauri::async_runtime::block_on(fetch_external_comments_with_state(
            action.link.object.id,
            &shared_state,
        ))
        .expect("local comments should load");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "A local note.");
        {
            let mut runtime = shared_state
                .lock()
                .expect("runtime lock should be available");
            let remote = runtime
                .register_machine(
                    1,
                    "Remote Machine".into(),
                    "remote-markdown".into(),
                    MachineTransport::Ssh {
                        host: "example.test".into(),
                        user: None,
                        port: None,
                        identity_file: None,
                        known_hosts_file: None,
                        strict_host_key_checking: None,
                    },
                )
                .expect("remote Machine should register");
            let error = runtime
                .set_context_execution_machine(1, Some(remote.id))
                .expect_err("local tracker Context must reject a remote Machine");
            assert!(error.contains("requires a local execution Machine"));
        }
        let state = shared_state
            .lock()
            .expect("runtime lock should be available");
        let snapshot = state
            .state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == action.link.object.id)
            .expect("local Spec should have a snapshot");
        assert_eq!(snapshot.state, "In Progress");
        assert!(
            state.state.worktrees.is_empty(),
            "local Spec reading should not require a Worktree"
        );
        drop(state);
        std::fs::remove_file(&spec_path).expect("test should remove the linked Spec");
        let error = tauri::async_runtime::block_on(fetch_issue_document_with_state(
            action.link.object.id,
            &shared_state,
        ))
        .expect_err("missing local files should return an explanatory error");
        assert!(error.contains("missing or unreadable"));
    }
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

        let context_id = project.context_id;
        let stored_path = self.configured_gh_path(context_id)?;
        let executable = resolve_gh_executable(stored_path.as_deref()).map_err(|error| {
            format!(
                "GitHub CLI for Context '{}' could not be resolved: {error}",
                project.context_id
            )
        })?;
        let dispatch = ProviderDispatch::new(ExternalProvider::GitHub, Some(executable.clone()));
        let created_url = dispatch
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
        let snapshot = match dispatch.fetch_snapshot(&object, current_unix_seconds()) {
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
        self.remember_context_gh_executable(context_id, &executable)
            .map_err(|error| format!("GitHub Issue was created at {created_url}, but its Context configuration could not be updated: {error}"))?;
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
}
