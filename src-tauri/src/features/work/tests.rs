use super::*;

use std::{path::Path, process::Command, sync::Mutex};

use tempfile::tempdir;

use crate::domain::{
    attention_entries, ExternalMetadata, ExternalObjectInput, ExternalObjectKind, ExternalProvider,
    ExternalSnapshotData,
};

fn runtime_with_grill_run(
    database: &Path,
    terminal: crate::terminal::FakeTerminalRuntime,
) -> (Runtime, i64) {
    use crate::domain::{
        decide, AgentKind, Event, GrillConfiguration, MachineTransport, WorkspaceRepositoryInput,
    };

    let mut runtime = Runtime::open_with_terminal_runtime(database, terminal)
        .expect("runtime should open with the fake Terminal Runtime");
    let machine = runtime
        .register_machine(
            1,
            "Test Machine".into(),
            "mission".into(),
            MachineTransport::Local,
        )
        .expect("Machine should register");
    runtime
        .set_context_execution_machine(1, Some(machine.id))
        .expect("Context should use the Machine");
    let repository = runtime
        .register_repository(
            1,
            "service".into(),
            "https://example.com/service.git".into(),
        )
        .expect("Repository should register");
    let item = runtime
        .create_item("Exercise Grill command serialization".into(), 1, 1)
        .expect("Item should be created");
    let workspace = runtime
        .create_workspace(
            item.id,
            vec![WorkspaceRepositoryInput {
                repository_id: repository.id,
                branch: "feature/grill-lock".into(),
                base_branch: "main".into(),
            }],
        )
        .expect("Workspace should be created");
    let configuration = GrillConfiguration {
        agent: AgentKind::Claude,
        model: "claude-sonnet-4-5".into(),
        effort: "high".into(),
    };
    let prompt = runtime
        .compose_grill_prompt(item.id, configuration.clone(), "Test prompt".into())
        .expect("Grill prompt should compose");
    let decision = decide(
        runtime.state.clone(),
        Event::StartGrillRun {
            item_id: item.id,
            workspace_id: workspace.id,
            repository_id: repository.id,
            machine_id: machine.id,
            configuration,
            prompt,
            skill_snapshot: crate::domain::GRILL_SKILL_SNAPSHOT.into(),
            working_directory: "/tmp/grill-lock-test".into(),
            session_name: "grill-lock-test".into(),
            pane_id: "%1".into(),
            started_at: 1,
            checkouts: vec![crate::domain::RunCheckout {
                repository_id: repository.id,
                path: "/tmp/grill-lock-test".into(),
                branch: "feature/grill-lock".into(),
                is_dirty: false,
            }],
        },
    )
    .expect("Grill Run should be created");
    runtime.commit(decision).expect("Grill Run should persist");
    let run_id = runtime
        .state
        .runs
        .last()
        .expect("Grill Run should remain available")
        .id;
    (runtime, run_id)
}

fn wait_for_grill_operation_waiter(state: &std::sync::Arc<Mutex<Runtime>>, run_id: i64) -> bool {
    use std::{thread, time::Duration};

    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        let operation_lock = state
            .lock()
            .expect("Runtime should remain available")
            .grill_operation_lock(run_id);
        // One owned guard holds the lock for the first request; the queued
        // request owns another Arc while it awaits the same per-Run lock.
        if std::sync::Arc::strong_count(&operation_lock) >= 3 {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn item_interface_preserves_lifecycle_notes_reminders_relations_and_status() {
    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut runtime = Runtime::open(&database).expect("runtime should open");

    runtime
        .create_item("Plan the handoff".into(), 1, 1)
        .expect("the first Item should be created");
    runtime
        .create_item("Prepare the release".into(), 1, 1)
        .expect("the second Item should be created");
    runtime
        .update_item(
            Event::SetItemNotes {
                item_id: 1,
                notes: "Keep the private context".into(),
            },
            1,
        )
        .expect("Item notes should be updated");
    runtime
        .update_item(
            Event::AddItemReminder {
                item_id: 1,
                remind_at: "2026-09-23T09:00:00Z".into(),
            },
            1,
        )
        .expect("Item reminders should be updated");
    runtime
        .set_item_relation(1, 2, ItemRelationKind::Blocks)
        .expect("the Item relation should be created");
    runtime
        .update_item(
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Active,
            },
            1,
        )
        .expect("the Item status should be updated");

    let item = runtime
        .state
        .items
        .iter()
        .find(|item| item.id == 1)
        .expect("the Item should remain available");
    assert_eq!(item.status, ItemStatus::Active);
    assert_eq!(item.notes, "Keep the private context");
    assert_eq!(item.reminders.len(), 1);
    assert_eq!(runtime.state.relationships.len(), 1);
}

#[test]
fn attention_interface_preserves_policy_watch_and_review_watermark() {
    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut runtime = Runtime::open(&database).expect("runtime should open");
    runtime
        .create_item("Review provider change".into(), 1, 1)
        .expect("the Item should be created");

    let link_decision = decide(
        runtime.state.clone(),
        Event::LinkExternalObject {
            item_id: 1,
            object: ExternalObjectInput {
                provider: ExternalProvider::Generic,
                kind: ExternalObjectKind::Generic,
                external_key: "provider-1".into(),
                canonical_url: "https://example.com/provider-1".into(),
            },
            snapshot: Some(ExternalSnapshotData {
                title: "Initial title".into(),
                state: "open".into(),
                metadata: vec![ExternalMetadata {
                    key: "owner".into(),
                    value: "team".into(),
                }],
                fetched_at: 1,
            }),
        },
    )
    .expect("the Link should be created through the state-transition seam");
    runtime
        .commit(link_decision)
        .expect("the Link should persist");

    let refresh_decision = decide(
        runtime.state.clone(),
        Event::RefreshExternalObject {
            external_object_id: 1,
            snapshot: ExternalSnapshotData {
                title: "Updated title".into(),
                state: "open".into(),
                metadata: vec![ExternalMetadata {
                    key: "owner".into(),
                    value: "team".into(),
                }],
                fetched_at: 2,
            },
        },
    )
    .expect("the changed snapshot should be accepted");
    runtime
        .commit(refresh_decision)
        .expect("the changed snapshot should persist");

    assert_eq!(attention_entries(&runtime.state, None, "2").len(), 1);
    let view = runtime
        .set_link_attention_policy(
            1,
            Some(ExternalChangePolicy {
                title: true,
                state: false,
                metadata: false,
            }),
        )
        .expect("the Link attention policy should update");
    assert!(view.attention_policy.title);
    assert!(!view.attention_policy.state);

    runtime
        .set_link_schedule(
            Event::SetLinkWatchUntil {
                link_id: 1,
                watch_until: Some("2026-09-21".into()),
            },
            1,
            "Setting watch period",
        )
        .expect("the watch period should update");
    assert!(attention_entries(&runtime.state, None, "2026-09-22").is_empty());

    runtime
        .set_link_schedule(
            Event::SetLinkWatchUntil {
                link_id: 1,
                watch_until: None,
            },
            1,
            "Setting watch period",
        )
        .expect("the watch period should clear");
    runtime
        .mark_link_reviewed(1)
        .expect("reviewing the Link should advance its watermark");
    assert!(attention_entries(&runtime.state, None, "2026-09-22").is_empty());
}

#[test]
fn startup_recovers_legacy_run_state_without_moving_or_deleting_the_file() {
    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut runtime = Runtime::open(&database).expect("runtime should open");
    runtime.state.machines.push(crate::domain::Machine {
        id: 1,
        context_id: 1,
        name: "Local Mac".into(),
        socket_name: "mission-test".into(),
        transport: crate::domain::MachineTransport::Local,
        last_observed: crate::domain::MachineObservation::Unknown,
        last_observed_at: None,
    });
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
        session_name: "legacy-run".into(),
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
    });

    let legacy_file = directory.path().join("agent-state/run-7.json");
    std::fs::create_dir_all(legacy_file.parent().unwrap()).unwrap();
    std::fs::write(
        &legacy_file,
        serde_json::to_vec(&AgentStateRecord {
            agent: crate::domain::AgentKind::Claude,
            run_id: "7".into(),
            state: crate::domain::RunState::Blocked,
            updated_at: "123".into(),
            sequence: None,
        })
        .unwrap(),
    )
    .unwrap();

    runtime
        .recover_run_states()
        .expect("legacy run state should recover");

    assert_eq!(
        runtime.state.runs[0].state,
        crate::domain::RunState::Blocked
    );
    let wrong_run = runtime
        .apply_agent_state_record(
            7,
            AgentStateRecord {
                agent: crate::domain::AgentKind::Claude,
                run_id: "8".into(),
                state: crate::domain::RunState::Finished,
                updated_at: "124".into(),
                sequence: Some(1),
            },
        )
        .expect("a report for another Run should be ignored");
    let wrong_agent = runtime
        .apply_agent_state_record(
            7,
            AgentStateRecord {
                agent: crate::domain::AgentKind::Codex,
                run_id: "7".into(),
                state: crate::domain::RunState::Finished,
                updated_at: "125".into(),
                sequence: Some(1),
            },
        )
        .expect("a report from another agent should be ignored");
    assert!(!wrong_run.accepted);
    assert!(!wrong_agent.accepted);
    assert_eq!(
        runtime.state.runs[0].state,
        crate::domain::RunState::Blocked
    );

    let apply_report = |runtime: &mut Runtime, sequence, state| {
        runtime.apply_agent_state_record(
            7,
            AgentStateRecord {
                agent: crate::domain::AgentKind::Claude,
                run_id: "7".into(),
                state,
                updated_at: "ignored for ordering".into(),
                sequence: Some(sequence),
            },
        )
    };
    let first = apply_report(&mut runtime, 1, crate::domain::RunState::Working)
        .expect("sequence 1 should apply");
    let third = apply_report(&mut runtime, 3, crate::domain::RunState::Finished)
        .expect("sequence 3 should apply");
    let stale = apply_report(&mut runtime, 2, crate::domain::RunState::Blocked)
        .expect("sequence 2 should be ignored");
    let duplicate = apply_report(&mut runtime, 3, crate::domain::RunState::Blocked)
        .expect("duplicate sequence 3 should be ignored");
    let fourth = apply_report(&mut runtime, 4, crate::domain::RunState::Finished)
        .expect("sequence 4 should update the applied marker");

    assert!(first.accepted && first.state_changed);
    assert!(first.run_changed);
    assert!(third.accepted && third.state_changed);
    assert!(third.run_changed);
    assert!(!stale.accepted && !stale.state_changed);
    assert!(!stale.run_changed);
    assert!(!duplicate.accepted && !duplicate.state_changed);
    assert!(!duplicate.run_changed);
    assert!(fourth.accepted && !fourth.state_changed);
    assert!(fourth.run_changed, "the accepted sequence marker changed");
    assert_eq!(
        runtime.state.runs[0].state,
        crate::domain::RunState::Finished
    );
    assert_eq!(
        runtime.state.runs[0].last_applied_agent_state_sequence,
        Some(4)
    );
    assert!(
        legacy_file.exists(),
        "recovery must leave the legacy file in place"
    );
}

#[test]
fn workspace_interface_keeps_repository_selection_and_worktree_identity() {
    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut runtime = Runtime::open(&database).expect("runtime should open");

    runtime
        .register_machine(
            1,
            "Local Mac".into(),
            "mission".into(),
            crate::domain::MachineTransport::Local,
        )
        .expect("Machine should be created");
    runtime
        .set_context_execution_machine(1, Some(1))
        .expect("the Context should use its registered Machine");
    runtime
        .register_repository(
            1,
            "service".into(),
            "https://example.com/service.git".into(),
        )
        .expect("Repository should be created");
    runtime
        .create_item("Prepare reusable work".into(), 1, 1)
        .expect("Item should be created");

    let workspace = runtime
        .create_workspace(
            1,
            vec![WorkspaceRepositoryInput {
                repository_id: 1,
                branch: "feature/api".into(),
                base_branch: "main".into(),
            }],
        )
        .expect("Workspace should be created");
    let worktree = runtime
        .create_worktree(
            workspace.id,
            1,
            1,
            "~/worktrees/workspace-1/feature-api/service".into(),
            "feature/api".into(),
            "main".into(),
        )
        .expect("Worktree should be created");

    assert_eq!(workspace.item_id, 1);
    assert_eq!(workspace.repositories[0].repository_id, 1);
    assert_eq!(worktree.workspace_id, workspace.id);
    assert_eq!(worktree.machine_id, 1);
    assert_eq!(worktree.branch, "feature/api");
    assert_eq!(worktree.base_branch, "main");
    assert!(!worktree.is_dirty);
    assert_eq!(
        runtime
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace.id)
            .expect("created Workspace should remain available")
            .preparation_state,
        crate::domain::WorkspacePreparationState::Ready
    );

    let reopened = Runtime::open(&database).expect("runtime should reopen");
    assert_eq!(reopened.state.workspaces, runtime.state.workspaces);
    assert_eq!(reopened.state.worktrees, runtime.state.worktrees);
}

#[test]
fn startup_preserves_workspace_branches_and_defaults_new_repositories() {
    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut runtime = Runtime::open(&database).expect("runtime should open");
    runtime
        .register_repository(
            1,
            "service-a".into(),
            "https://example.com/service-a.git".into(),
        )
        .expect("first Repository should register");
    runtime
        .create_item("Preserve selected branches".into(), 1, 1)
        .expect("Item should be created");
    let workspace = runtime
        .create_workspace(
            1,
            vec![WorkspaceRepositoryInput {
                repository_id: 1,
                branch: "feature/api".into(),
                base_branch: "release/1".into(),
            }],
        )
        .expect("Workspace should be created");
    runtime
        .register_repository(
            1,
            "service-b".into(),
            "https://example.com/service-b.git".into(),
        )
        .expect("second Repository should register");

    let reopened = Runtime::open(&database).expect("runtime should reopen");
    let reopened_workspace = reopened
        .state
        .workspaces
        .iter()
        .find(|candidate| candidate.id == workspace.id)
        .expect("Workspace should survive reopening");
    assert_eq!(
        reopened_workspace.repositories,
        vec![
            crate::domain::WorkspaceRepository {
                repository_id: 1,
                branch: "feature/api".into(),
                base_branch: "release/1".into(),
            },
            crate::domain::WorkspaceRepository {
                repository_id: 2,
                branch: "mission-MC-1".into(),
                base_branch: "main".into(),
            },
        ]
    );
}

#[cfg(unix)]
#[test]
fn direct_run_preview_inspects_checkouts_before_returning_the_existing_contract() {
    use crate::domain::{MachineTransport, RepositoryLocation};

    let directory = tempdir().expect("temporary app directory should exist");
    let checkout = directory.path().join("service");
    std::fs::create_dir_all(&checkout).expect("checkout directory should exist");
    let run_git = |arguments: &[&str]| {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&checkout)
            .output()
            .expect("Git should run");
        assert!(
            output.status.success(),
            "Git command should succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run_git(&["init", "--quiet", "-b", "main"]);
    run_git(&["config", "user.name", "Mission Manager Test"]);
    run_git(&["config", "user.email", "mission-manager@example.test"]);
    std::fs::write(checkout.join("README.md"), "service\n")
        .expect("checkout file should be written");
    run_git(&["add", "README.md"]);
    run_git(&["commit", "--quiet", "-m", "initial"]);

    let database = directory.path().join("mission-manager.sqlite");
    let mut runtime = Runtime::open(&database).expect("runtime should open");
    runtime
        .register_machine(
            1,
            "Local Mac".into(),
            "mission".into(),
            MachineTransport::Local,
        )
        .expect("Machine should be registered");
    runtime
        .set_context_execution_machine(1, Some(1))
        .expect("Context should use its Machine");
    runtime
        .register_repository(
            1,
            "service".into(),
            "https://github.com/acme/service.git".into(),
        )
        .expect("Repository should be registered");
    runtime
        .create_item("Inspect service".into(), 1, 1)
        .expect("Item should be created");
    let workspace = runtime
        .create_workspace(
            1,
            vec![WorkspaceRepositoryInput {
                repository_id: 1,
                branch: "mission-1".into(),
                base_branch: "main".into(),
            }],
        )
        .expect("Workspace should be created");
    runtime.state.repository_locations.push(RepositoryLocation {
        repository_id: 1,
        machine_id: 1,
        checkout_path: checkout.to_string_lossy().into_owned(),
        worktree_root: directory
            .path()
            .join("worktrees")
            .to_string_lossy()
            .into_owned(),
    });
    let state = Mutex::new(runtime);

    let preview = tauri::async_runtime::block_on(runs::prepare_direct_run_with_state(
        1,
        workspace.id,
        None,
        &state,
    ))
    .expect("Direct Run preview should inspect the checkout");

    assert_eq!(preview.workspace_id, workspace.id);
    assert_eq!(preview.machine_id, 1);
    assert_eq!(preview.current_branches, vec!["main"]);
    assert_eq!(preview.checkout_details[0].repository_id, 1);
    assert_eq!(preview.checkout_details[0].path, checkout.to_string_lossy());
    assert!(!preview.checkout_details[0].is_dirty);
}

#[test]
fn a_failed_run_preflight_never_calls_the_agent_launcher() {
    use crate::{
        domain::{AgentKind, ExecutionProfile, MachineTransport, RunPromptSelection, Worktree},
        terminal::{FakeMachineOutcome, FakeTerminalCommand, FakeTerminalRuntime},
    };

    for outcome in [
        FakeMachineOutcome::Unreachable,
        FakeMachineOutcome::TmuxUnavailable,
        FakeMachineOutcome::AgentUnavailable,
        FakeMachineOutcome::StateDirectoryUnwritable,
        FakeMachineOutcome::HookProvisioningFailed,
    ] {
        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let fake = FakeTerminalRuntime::new([(1, outcome)]);
        let commands = fake.command_log();
        let mut runtime = Runtime::open_with_terminal_runtime(&database, fake)
            .expect("runtime should open with the fake Terminal Runtime");
        runtime
            .register_machine(
                1,
                "Test Machine".into(),
                "mission".into(),
                MachineTransport::Ssh {
                    host: "build.example".into(),
                    user: Some("runner".into()),
                    port: None,
                    identity_file: None,
                    known_hosts_file: None,
                    strict_host_key_checking: None,
                },
            )
            .expect("Machine should be registered");
        runtime
            .set_context_execution_machine(1, Some(1))
            .expect("Context should use the Machine");
        runtime
            .register_repository(
                1,
                "service".into(),
                "https://example.com/service.git".into(),
            )
            .expect("Repository should be registered");
        let item = runtime
            .create_item("Start a Run".into(), 1, 1)
            .expect("Item should be created");
        let workspace = runtime
            .create_workspace(
                item.id,
                vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "feature/preflight".into(),
                    base_branch: "main".into(),
                }],
            )
            .expect("execution setup should be created");
        runtime.state.worktrees.push(Worktree {
            id: 1,
            workspace_id: workspace.id,
            repository_id: 1,
            machine_id: 1,
            path: directory.path().to_string_lossy().into_owned(),
            branch: "feature/preflight".into(),
            base_branch: "main".into(),
            is_dirty: false,
        });

        let error = runtime
            .start_worktree_run(
                item.id,
                workspace.id,
                1,
                AgentKind::Claude,
                ExecutionProfile::Implement,
                "Implement the change".into(),
                RunPromptSelection {
                    include_objective: true,
                    include_notes: false,
                    external_object_ids: Vec::new(),
                },
            )
            .expect_err("a failed preflight should abort the Run");
        assert!(error.contains("Run preflight failed on Machine Test Machine"));
        assert!(error.contains("not started locally"));
        let recorded = commands
            .lock()
            .expect("fake command log should remain available")
            .clone();
        assert!(recorded.iter().any(|command| matches!(
            command,
            FakeTerminalCommand::PreflightAgentRun {
                machine_id: 1,
                agent: AgentKind::Claude,
                run_id: 1,
            }
        )));
        assert!(!recorded
            .iter()
            .any(|command| matches!(command, FakeTerminalCommand::LaunchAgent { .. })));
    }
}

#[test]
fn machine_check_returns_per_agent_hook_failures_for_settings() {
    use crate::{
        domain::MachineTransport,
        terminal::{FakeMachineOutcome, FakeTerminalRuntime},
    };

    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let fake = FakeTerminalRuntime::new([(1, FakeMachineOutcome::HookProvisioningFailed)]);
    let mut runtime = Runtime::open_with_terminal_runtime(&database, fake)
        .expect("runtime should open with the fake Terminal Runtime");
    runtime
        .register_machine(
            1,
            "Remote Machine".into(),
            "mission".into(),
            MachineTransport::Ssh {
                host: "build.example".into(),
                user: Some("runner".into()),
                port: None,
                identity_file: None,
                known_hosts_file: None,
                strict_host_key_checking: None,
            },
        )
        .expect("Machine should be registered");

    let runtime = std::sync::Mutex::new(runtime);
    let checked = tauri::async_runtime::block_on(
        crate::features::structure::check_machine_with_state(1, &runtime),
    )
    .expect("hook provisioning failures should be represented in the readiness result");
    let readiness = checked
        .readiness
        .expect("Settings should receive Machine readiness");
    assert_eq!(readiness.claude_hooks.provisioned, Some(false));
    assert_eq!(readiness.claude_hooks.current, Some(false));
    assert_eq!(readiness.codex_hooks.provisioned, Some(false));
    assert_eq!(readiness.codex_hooks.current, Some(false));
    assert_eq!(
        readiness.last_provisioning_error.as_deref(),
        Some("fake hook provisioning failed")
    );
}

#[test]
fn settings_check_reports_hooks_even_when_tmux_is_unavailable() {
    use crate::{
        domain::MachineTransport,
        terminal::{FakeMachineOutcome, FakeTerminalRuntime},
    };

    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let fake = FakeTerminalRuntime::new([(1, FakeMachineOutcome::TmuxUnavailable)]);
    let mut runtime = Runtime::open_with_terminal_runtime(&database, fake)
        .expect("runtime should open with the fake Terminal Runtime");
    runtime
        .register_machine(
            1,
            "Remote Machine".into(),
            "mission".into(),
            MachineTransport::Ssh {
                host: "build.example".into(),
                user: Some("runner".into()),
                port: None,
                identity_file: None,
                known_hosts_file: None,
                strict_host_key_checking: None,
            },
        )
        .expect("Machine should be registered");

    let runtime = std::sync::Mutex::new(runtime);
    let checked = tauri::async_runtime::block_on(
        crate::features::structure::check_machine_with_state(1, &runtime),
    )
    .expect("Machine checks should return readiness even when tmux is missing");
    let readiness = checked.readiness.expect("readiness should be returned");
    assert_eq!(readiness.tmux_available, Some(false));
    assert_eq!(readiness.claude_hooks.provisioned, Some(true));
    assert_eq!(readiness.claude_hooks.current, Some(true));
    assert_eq!(readiness.codex_hooks.provisioned, Some(true));
    assert_eq!(readiness.codex_hooks.current, Some(true));
}

#[test]
fn run_suggestion_tmux_inspection_does_not_hold_the_runtime_lock() {
    use crate::{
        domain::MachineTransport,
        terminal::{FakeMachineOutcome, FakeTerminalRuntime},
    };
    use std::{sync::Arc, thread, time::Duration};

    let directory = tempdir().expect("temporary app directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let fake = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
    let blocked_observation = fake.block_observations();
    let mut runtime = Runtime::open_with_terminal_runtime(&database, fake)
        .expect("runtime should open with the fake Terminal Runtime");
    runtime
        .register_machine(
            1,
            "Test Machine".into(),
            "mission".into(),
            MachineTransport::Local,
        )
        .expect("Machine should register");
    runtime
        .set_context_execution_machine(1, Some(1))
        .expect("Context should use the Machine");

    let state = Arc::new(Mutex::new(runtime));
    let worker_state = Arc::clone(&state);
    let worker = thread::spawn(move || {
        tauri::async_runtime::block_on(runs::list_run_suggestions_with_state(&worker_state))
    });

    assert!(blocked_observation.wait_until_started(Duration::from_secs(2)));
    state
        .lock()
        .expect("Runtime should remain lockable while tmux is blocked")
        .create_item("Local state change during tmux inspection".into(), 1, 1)
        .expect("local state change should complete during tmux inspection");
    blocked_observation.release();

    let error = worker
        .join()
        .expect("suggestion worker should finish")
        .expect_err("stale suggestions should be rejected after local state changes");
    assert!(error.contains("state changed while agent Panes were inspected"));
}

#[test]
fn concurrent_grill_answer_submissions_send_only_once() {
    use crate::domain::{decide, Event, GrillAnswer, RunState};
    use crate::terminal::{FakeMachineOutcome, FakeTerminalCommand, FakeTerminalRuntime};
    use std::{
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    let directory = tempdir().expect("temporary app directory should exist");
    let fake = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
    let blocked_send = fake.block_sends();
    let commands = fake.command_log();
    let (mut runtime, run_id) =
        runtime_with_grill_run(&directory.path().join("mission-manager.sqlite"), fake);
    let question_group = parse_grill_question_group("❓ Q1: Keep this decision?")
        .expect("question group should parse");
    runtime
        .commit(
            decide(
                runtime.state.clone(),
                Event::RecordRunTranscript {
                    run_id,
                    transcript: "❓ Q1: Keep this decision?".into(),
                    question_group: Some(question_group),
                },
            )
            .expect("question group should be recorded"),
        )
        .expect("question group should persist");
    runtime
        .commit(
            decide(
                runtime.state.clone(),
                Event::UpdateRunState {
                    run_id,
                    state: RunState::Blocked,
                },
            )
            .expect("Run should enter the answer phase"),
        )
        .expect("answer phase should persist");
    let state = Arc::new(Mutex::new(runtime));
    let first_state = Arc::clone(&state);
    let answer = GrillAnswer {
        question_number: 1,
        answer: "Keep it".into(),
    };
    let first = thread::spawn(move || {
        tauri::async_runtime::block_on(runs::submit_grill_answers_with_state(
            run_id,
            vec![answer],
            &first_state,
        ))
    });
    assert!(blocked_send.wait_until_started(Duration::from_secs(2)));

    let second_state = Arc::clone(&state);
    let (started_sender, started_receiver) = mpsc::channel();
    let second = thread::spawn(move || {
        started_sender
            .send(())
            .expect("test should receive the second request start");
        tauri::async_runtime::block_on(runs::submit_grill_answers_with_state(
            run_id,
            vec![GrillAnswer {
                question_number: 1,
                answer: "Keep it".into(),
            }],
            &second_state,
        ))
    });
    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("second request should start");
    assert!(wait_for_grill_operation_waiter(&state, run_id));
    let send_count = commands
        .lock()
        .expect("fake command log should remain available")
        .iter()
        .filter(|command| matches!(command, FakeTerminalCommand::SendPaneInput { .. }))
        .count();
    assert_eq!(send_count, 1, "queued answer should not be sent early");

    blocked_send.release();
    first
        .join()
        .expect("first request should finish")
        .expect("first answer submission should succeed");
    let error = second
        .join()
        .expect("second request should finish")
        .expect_err("second answer submission should observe the completed first request");
    assert!(error.contains("not waiting for Grill answers"));
    let send_count = commands
        .lock()
        .expect("fake command log should remain available")
        .iter()
        .filter(|command| matches!(command, FakeTerminalCommand::SendPaneInput { .. }))
        .count();
    assert_eq!(send_count, 1);
}

#[test]
fn concurrent_grill_continuations_send_only_once() {
    use crate::domain::{decide, Event, GrillContinuationAction, RunState};
    use crate::terminal::{FakeMachineOutcome, FakeTerminalCommand, FakeTerminalRuntime};
    use std::{
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    let directory = tempdir().expect("temporary app directory should exist");
    let fake = FakeTerminalRuntime::new([(1, FakeMachineOutcome::Available)]);
    let blocked_send = fake.block_sends();
    let commands = fake.command_log();
    let (mut runtime, run_id) =
        runtime_with_grill_run(&directory.path().join("mission-manager.sqlite"), fake);
    runtime
        .commit(
            decide(
                runtime.state.clone(),
                Event::UpdateRunState {
                    run_id,
                    state: RunState::Finished,
                },
            )
            .expect("Grill Run should reach its continuation frontier"),
        )
        .expect("continuation frontier should persist");
    let state = Arc::new(Mutex::new(runtime));
    let first_state = Arc::clone(&state);
    let first = thread::spawn(move || {
        tauri::async_runtime::block_on(runs::continue_grill_with_state(
            run_id,
            GrillContinuationAction::ToSpec,
            &first_state,
        ))
    });
    assert!(blocked_send.wait_until_started(Duration::from_secs(2)));

    let second_state = Arc::clone(&state);
    let (started_sender, started_receiver) = mpsc::channel();
    let second = thread::spawn(move || {
        started_sender
            .send(())
            .expect("test should receive the second request start");
        tauri::async_runtime::block_on(runs::continue_grill_with_state(
            run_id,
            GrillContinuationAction::ToSpec,
            &second_state,
        ))
    });
    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("second request should start");
    assert!(wait_for_grill_operation_waiter(&state, run_id));
    let send_count = commands
        .lock()
        .expect("fake command log should remain available")
        .iter()
        .filter(|command| matches!(command, FakeTerminalCommand::SendPaneInput { .. }))
        .count();
    assert_eq!(send_count, 1, "queued continuation should not send early");

    blocked_send.release();
    first
        .join()
        .expect("first request should finish")
        .expect("first continuation should succeed");
    let error = second
        .join()
        .expect("second request should finish")
        .expect_err("second continuation should observe the completed first request");
    assert!(
        !error.is_empty(),
        "queued continuation should be rejected: {error}"
    );
    let send_count = commands
        .lock()
        .expect("fake command log should remain available")
        .iter()
        .filter(|command| matches!(command, FakeTerminalCommand::SendPaneInput { .. }))
        .count();
    assert_eq!(send_count, 1);
}
