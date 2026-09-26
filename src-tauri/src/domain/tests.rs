use super::*;

mod implementation_queue_tests {
    use super::*;

    fn state() -> DomainState {
        DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 2,
            next_link_id: 2,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Context".into(),
                execution_machine_id: Some(1),
                grill_defaults: GrillConfiguration::default(),
                implement_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Project".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "repo".into(),
                remote_url: "git@github.com:o/r.git".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![],
            items: vec![Item {
                id: 1,
                human_identifier: "ITEM-1".into(),
                title: "Work".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: vec![],
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "feature".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Ready,
            }],
            worktrees: vec![],
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Mac".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: vec![],
            implementation_queues: vec![],
            relationships: vec![],
            external_objects: vec![ExternalObject {
                id: 1,
                provider: ExternalProvider::GitHub,
                kind: ExternalObjectKind::Issue,
                external_key: "o/r#87".into(),
                canonical_url: "https://github.com/o/r/issues/87".into(),
            }],
            links: vec![Link {
                id: 1,
                item_id: 1,
                external_object_id: 1,
                reviewed_activity_id: 0,
                attention_policy: None,
                watch_until: None,
                review_at: None,
                provenance: None,
            }],
            snapshots: vec![],
            activities: vec![],
            attention_defaults: vec![],
        }
    }

    fn event(ticket_state: &str) -> Event {
        Event::StartDirectRun {
            item_id: 1,
            workspace_id: 1,
            machine_id: 1,
            agent: AgentKind::Claude,
            configuration: Some(GrillConfiguration::default()),
            execution_profile: ExecutionProfile::Implement,
            prompt: "ignored by queue projection".into(),
            working_directory: "/repo".into(),
            session_name: "session".into(),
            pane_id: "%1".into(),
            started_at: 1,
            prompt_selection: RunPromptSelection {
                include_objective: true,
                include_notes: false,
                external_object_ids: vec![],
            },
            checkouts: vec![RunCheckout {
                repository_id: 1,
                path: "/repo".into(),
                branch: "feature".into(),
                is_dirty: false,
            }],
            repository_id: 1,
            allow_dirty: false,
            allow_shared_checkouts: false,
            implementation_queue: Some(ImplementationQueueStart {
                spec_external_object_id: 1,
                spec_url: "https://github.com/o/r/issues/87".into(),
                entries: vec![
                    ImplementationQueueEntry {
                        position: 0,
                        ticket_number: 89,
                        ticket_title: "Implement queue".into(),
                        ticket_url: "https://github.com/o/r/issues/89".into(),
                        ticket_state: ticket_state.into(),
                        run_id: None,
                        done: false,
                        skipped: false,
                    },
                    ImplementationQueueEntry {
                        position: 1,
                        ticket_number: 90,
                        ticket_title: "Later".into(),
                        ticket_url: "https://github.com/o/r/issues/90".into(),
                        ticket_state: "open".into(),
                        run_id: None,
                        done: false,
                        skipped: false,
                    },
                ],
            }),
        }
    }

    #[test]
    fn starts_only_first_queue_entry_and_persists_queue_with_run() {
        let decision = decide(state(), event("open")).unwrap();
        assert_eq!(
            decision.state.implementation_queues[0].entries[0].run_id,
            Some(1)
        );
        assert!(decision
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PersistImplementationQueue { .. })));
        assert_eq!(decision.state.runs.len(), 1);
        assert_eq!(
            decision.state.runs[0].execution_profile,
            ExecutionProfile::Implement
        );
        assert_eq!(
            decision.state.runs[0].model.as_deref(),
            Some("claude-sonnet-4-5")
        );
        assert_eq!(decision.state.runs[0].effort.as_deref(), Some("high"));
        assert_eq!(
            decision
                .effects
                .iter()
                .filter(|effect| matches!(effect, Effect::PersistRun { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn completed_ticket_and_clean_checkout_advance_and_finish_without_changing_item() {
        let started = decide(state(), event("open")).unwrap();
        let finished = decide(
            started.state,
            Event::ApplyAgentStateReport {
                run_id: 1,
                state: RunState::Finished,
                sequence: Some(1),
            },
        )
        .unwrap();
        assert!(finished.effects.iter().any(|effect| matches!(
            effect,
            Effect::FetchImplementationTicketState {
                queue_id: 1,
                run_id: 1,
                ..
            }
        )));
        assert!(finished.effects.iter().any(|effect| matches!(
            effect,
            Effect::InspectImplementationCheckout {
                queue_id: 1,
                run_id: 1,
                ..
            }
        )));

        let first_done = decide(
            finished.state,
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 1,
                ticket_closed: true,
                checkout_clean: true,
            },
        )
        .unwrap();
        assert!(first_done.state.implementation_queues[0].entries[0].done);
        assert!(first_done.state.implementation_queues[0].active);
        assert!(first_done
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::CloseImplementationRunSession { run_id: 1 })));
        assert!(first_done.effects.iter().any(|effect| matches!(
            effect,
            Effect::LaunchImplementationQueueEntry {
                queue_id: 1,
                position: 1
            }
        )));

        let mut next_run = event("open");
        if let Event::StartDirectRun {
            implementation_queue,
            session_name,
            pane_id,
            ..
        } = &mut next_run
        {
            *implementation_queue = None;
            *session_name = "session-2".into();
            *pane_id = "%2".into();
        }
        let launched = decide(first_done.state, next_run).unwrap();
        let attached = decide(
            launched.state,
            Event::SetImplementationQueueEntryRun {
                queue_id: 1,
                position: 1,
                run_id: 2,
            },
        )
        .unwrap();
        let finished = decide(
            attached.state,
            Event::ApplyAgentStateReport {
                run_id: 2,
                state: RunState::Finished,
                sequence: Some(1),
            },
        )
        .unwrap();
        let completed = decide(
            finished.state,
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 2,
                ticket_closed: true,
                checkout_clean: true,
            },
        )
        .unwrap();
        assert!(completed.state.implementation_queues[0]
            .entries
            .iter()
            .all(|entry| entry.done));
        assert!(!completed.state.implementation_queues[0].active);
        assert_eq!(completed.state.items[0].status, ItemStatus::Active);
        assert!(!completed
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PersistItemUpdate { .. })));
        assert!(!completed
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::LaunchImplementationQueueEntry { .. })));
    }

    #[test]
    fn rejects_closed_ticket_and_second_active_queue() {
        assert!(matches!(
            decide(state(), event("closed")),
            Err(DomainError::InvalidImplementationQueueEntries)
        ));
        let first = decide(state(), event("open")).unwrap();
        let mut second = event("open");
        if let Event::StartDirectRun {
            session_name,
            pane_id,
            ..
        } = &mut second
        {
            *session_name = "session-2".into();
            *pane_id = "%2".into();
        }
        assert!(matches!(
            decide(first.state, second),
            Err(DomainError::ImplementationQueueAlreadyActive { item_id: 1 })
        ));
    }

    #[test]
    fn each_queue_pause_reason_raises_ticket_attention_and_clears_on_resume() {
        for reason in [
            ImplementationQueuePauseReason::TicketStillOpen,
            ImplementationQueuePauseReason::CheckoutDirty,
            ImplementationQueuePauseReason::RunStopped,
            ImplementationQueuePauseReason::PaneMissing,
            ImplementationQueuePauseReason::LaunchFailed("could not launch".into()),
        ] {
            let started = decide(state(), event("open")).unwrap();
            let paused = decide(
                started.state,
                Event::PauseImplementationQueue {
                    queue_id: 1,
                    reason: reason.clone(),
                },
            )
            .unwrap();
            assert_eq!(
                paused.state.implementation_queues[0].paused_reason,
                Some(reason)
            );
            let attention = attention_entries(&paused.state, None, "999");
            assert_eq!(attention.len(), 1);
            assert_eq!(attention[0].kind, AttentionEntryKind::ImplementationQueue);
            assert!(attention[0].summary.contains("#89"));
            assert!(attention[0].summary.contains("paused"));
            let resumed = decide(
                paused.state,
                Event::CancelImplementationQueue { queue_id: 1 },
            )
            .unwrap();
            assert!(attention_entries(&resumed.state, None, "999").is_empty());
        }
    }

    #[test]
    fn ticket_open_and_dirty_checkout_pause_with_specific_reasons() {
        let started = decide(state(), event("open")).unwrap();
        let finished = decide(
            started.state,
            Event::ApplyAgentStateReport {
                run_id: 1,
                state: RunState::Finished,
                sequence: Some(1),
            },
        )
        .unwrap();
        let still_open = decide(
            finished.state.clone(),
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 1,
                ticket_closed: false,
                checkout_clean: true,
            },
        )
        .unwrap();
        assert_eq!(
            still_open.state.implementation_queues[0].paused_reason,
            Some(ImplementationQueuePauseReason::TicketStillOpen)
        );
        let dirty = decide(
            finished.state,
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 1,
                ticket_closed: true,
                checkout_clean: false,
            },
        )
        .unwrap();
        assert_eq!(
            dirty.state.implementation_queues[0].paused_reason,
            Some(ImplementationQueuePauseReason::CheckoutDirty)
        );
    }

    #[test]
    fn automatic_resume_clears_pause_and_keeps_paused_run_session_open() {
        let started = decide(state(), event("open")).unwrap();
        let finished = decide(
            started.state,
            Event::ApplyAgentStateReport {
                run_id: 1,
                state: RunState::Finished,
                sequence: Some(1),
            },
        )
        .unwrap();
        let paused = decide(
            finished.state,
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 1,
                ticket_closed: false,
                checkout_clean: true,
            },
        )
        .unwrap();
        let resumed = decide(
            paused.state,
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 1,
                ticket_closed: true,
                checkout_clean: true,
            },
        )
        .unwrap();
        assert!(resumed.state.implementation_queues[0]
            .paused_reason
            .is_none());
        assert!(attention_entries(&resumed.state, None, "999").is_empty());
        assert!(!resumed
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::CloseImplementationRunSession { .. })));
        assert!(resumed.effects.iter().any(|effect| matches!(
            effect,
            Effect::LaunchImplementationQueueEntry { position: 1, .. }
        )));
    }

    #[test]
    fn check_again_cannot_advance_a_paused_run_before_its_turn_ends() {
        let started = decide(state(), event("open")).unwrap();
        let mut paused = decide(
            started.state,
            Event::PauseImplementationQueue {
                queue_id: 1,
                reason: ImplementationQueuePauseReason::RunStopped,
            },
        )
        .unwrap()
        .state;
        paused.runs[0].state = RunState::Working;

        let result = decide(
            paused,
            Event::AdvanceImplementationQueue {
                queue_id: 1,
                run_id: 1,
                ticket_closed: true,
                checkout_clean: true,
            },
        );

        assert!(matches!(
            result,
            Err(DomainError::ImplementationQueueRunNotFinished {
                queue_id: 1,
                run_id: 1
            })
        ));
    }

    #[test]
    fn skip_and_cancel_do_not_close_or_stop_paused_runs() {
        let started = decide(state(), event("open")).unwrap();
        let paused = decide(
            started.state,
            Event::PauseImplementationQueue {
                queue_id: 1,
                reason: ImplementationQueuePauseReason::RunStopped,
            },
        )
        .unwrap();
        let skipped = decide(
            paused.state.clone(),
            Event::SkipImplementationQueueEntry {
                queue_id: 1,
                position: 0,
            },
        )
        .unwrap();
        assert!(skipped.state.implementation_queues[0].entries[0].skipped);
        assert!(skipped.effects.iter().any(|effect| matches!(
            effect,
            Effect::LaunchImplementationQueueEntry { position: 1, .. }
        )));
        assert!(!skipped
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::CloseImplementationRunSession { .. })));
        let cancelled = decide(
            paused.state,
            Event::CancelImplementationQueue { queue_id: 1 },
        )
        .unwrap();
        assert!(!cancelled.state.implementation_queues[0].active);
        assert!(cancelled.state.implementation_queues[0]
            .paused_reason
            .is_none());
        assert!(!cancelled
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::CloseImplementationRunSession { .. })));
    }
}

mod context_execution_machine_tests {
    use super::*;

    fn state() -> DomainState {
        DomainState {
            next_context_id: 3,
            next_project_id: 1,
            next_item_id: 1,
            next_item_number: 1,
            next_repository_id: 1,
            next_workspace_id: 1,
            next_worktree_id: 1,
            next_machine_id: 3,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![
                Context {
                    id: 1,
                    name: "Unconfigured".into(),
                    execution_machine_id: None,
                    grill_defaults: GrillConfiguration::default(),
                    implement_defaults: GrillConfiguration::default(),
                },
                Context {
                    id: 2,
                    name: "Other".into(),
                    execution_machine_id: None,
                    grill_defaults: GrillConfiguration::default(),
                    implement_defaults: GrillConfiguration::default(),
                },
            ],
            projects: Vec::new(),
            repositories: Vec::new(),
            repository_locations: Vec::new(),
            items: Vec::new(),
            workspaces: Vec::new(),
            worktrees: Vec::new(),
            machines: vec![
                Machine {
                    id: 1,
                    context_id: 1,
                    name: "Build Mac".into(),
                    socket_name: "mission".into(),
                    transport: MachineTransport::Local,
                    last_observed: MachineObservation::Unknown,
                    last_observed_at: None,
                },
                Machine {
                    id: 2,
                    context_id: 2,
                    name: "Remote".into(),
                    socket_name: "mission".into(),
                    transport: MachineTransport::Local,
                    last_observed: MachineObservation::Unknown,
                    last_observed_at: None,
                },
            ],
            runs: Vec::new(),
            implementation_queues: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }

    #[test]
    fn a_context_can_be_configured_with_one_execution_machine_or_left_unconfigured() {
        let configured = decide(
            state(),
            Event::SetContextExecutionMachine {
                context_id: 1,
                machine_id: Some(1),
            },
        )
        .expect("a Context should be able to select one of its Machines");

        assert_eq!(configured.state.contexts[0].execution_machine_id, Some(1));

        let unconfigured = decide(
            configured.state,
            Event::SetContextExecutionMachine {
                context_id: 1,
                machine_id: None,
            },
        )
        .expect("a Context should be allowed to remain unconfigured");

        assert_eq!(unconfigured.state.contexts[0].execution_machine_id, None);
    }

    #[test]
    fn a_context_cannot_select_a_machine_owned_by_another_context() {
        let error = decide(
            state(),
            Event::SetContextExecutionMachine {
                context_id: 1,
                machine_id: Some(2),
            },
        )
        .expect_err("execution Machines must belong to their Context");

        assert!(matches!(error, DomainError::MachineContextMismatch { .. }));
    }
}

mod machine_deletion_tests {
    use super::*;

    fn state() -> DomainState {
        DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 2,
            next_machine_id: 2,
            next_run_id: 2,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                execution_machine_id: Some(1),
                grill_defaults: GrillConfiguration::default(),
                implement_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![RepositoryLocation {
                repository_id: 1,
                machine_id: 1,
                checkout_path: "/tmp/checkout".into(),
                worktree_root: "/tmp/worktrees".into(),
            }],
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Use the Context Machine".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "mission-MC-1".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Ready,
            }],
            worktrees: vec![Worktree {
                id: 1,
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                path: "/tmp/worktrees/mission-MC-1".into(),
                branch: "mission-MC-1".into(),
                base_branch: "main".into(),
                is_dirty: true,
            }],
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Build Mac".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: vec![Run {
                id: 1,
                item_id: 1,
                workspace_id: Some(1),
                repository_id: Some(1),
                worktree_id: Some(1),
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Implement,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt: "Implement this".into(),
                working_directory: "/tmp/worktrees/mission-MC-1".into(),
                session_name: "mission-item-1-run-1".into(),
                pane_id: "%1".into(),
                started_at: 1,
                state: RunState::Working,
                last_applied_agent_state_sequence: None,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
                grill_action_started_at: None,
            }],
            implementation_queues: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }

    #[test]
    fn deleting_a_machine_clears_its_context_and_all_app_owned_records_even_with_active_runs() {
        let state = state();
        let plan = plan_machine_deletion(&state, 1).expect("the Machine should be planned");
        assert_eq!(plan.active_run_ids, vec![1]);
        assert_eq!(plan.worktree_ids, vec![1]);
        assert_eq!(plan.repository_location_repository_ids, vec![1]);

        let decision = decide(
            state,
            Event::DeleteMachine {
                machine_id: 1,
                run_ids: vec![1],
                worktree_ids: vec![1],
                repository_location_repository_ids: vec![1],
            },
        )
        .expect("Machine deletion should proceed while best-effort stopping its Runs");

        assert!(decision.state.machines.is_empty());
        assert!(decision.state.runs.is_empty());
        assert!(decision.state.worktrees.is_empty());
        assert!(decision.state.repository_locations.is_empty());
        assert_eq!(decision.state.contexts[0].execution_machine_id, None);
        assert_eq!(
            decision.state.workspaces[0].preparation_state,
            WorkspacePreparationState::Pending
        );
        assert!(decision
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::RemoveRun { run_id: 1 })));
        assert!(decision
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::RemoveWorktree { worktree_id: 1 })));
    }

    #[test]
    fn machine_deletion_requires_the_previewed_worktree_records() {
        let error = decide(
            state(),
            Event::DeleteMachine {
                machine_id: 1,
                run_ids: vec![1],
                worktree_ids: Vec::new(),
                repository_location_repository_ids: vec![1],
            },
        )
        .expect_err("unreviewed Worktree records must not be deleted");

        assert!(matches!(
            error,
            DomainError::MachineWorktreesMismatch { .. }
        ));
    }

    #[test]
    fn changing_a_context_machine_is_blocked_while_its_run_is_active() {
        let error = decide(
            state(),
            Event::SetContextExecutionMachine {
                context_id: 1,
                machine_id: None,
            },
        )
        .expect_err("active Runs must finish before changing their Context Machine");

        assert!(matches!(error, DomainError::ContextHasActiveRuns { .. }));
    }

    #[test]
    fn changing_a_context_machine_keeps_old_worktrees_on_their_machine() {
        let mut state = state();
        state.runs.clear();
        state.machines.push(Machine {
            id: 2,
            context_id: 1,
            name: "New Build Mac".into(),
            socket_name: "mission-new".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Available,
            last_observed_at: None,
        });

        let decision = decide(
            state,
            Event::SetContextExecutionMachine {
                context_id: 1,
                machine_id: Some(2),
            },
        )
        .expect("Machine changes should not move existing Worktrees");

        assert_eq!(decision.state.contexts[0].execution_machine_id, Some(2));
        assert_eq!(decision.state.worktrees[0].machine_id, 1);
        assert_eq!(decision.state.machines[0].id, 1);
    }
}

mod workspace_contract_tests {
    use super::*;

    #[test]
    fn a_workspace_selects_repositories_and_worktree_execution_keeps_context() {
        let mut state = DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 1,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                execution_machine_id: Some(1),
                grill_defaults: GrillConfiguration::default(),
                implement_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: Vec::new(),
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Use Workspace vocabulary".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: Vec::new(),
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            implementation_queues: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let workspace = decide(
            state.clone(),
            Event::CreateWorkspace {
                item_id: 1,
                repositories: vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "feature/contract".into(),
                    base_branch: "main".into(),
                }],
            },
        )
        .expect("Workspace creation should succeed");
        state = workspace.state;
        assert_eq!(state.workspaces[0].item_id, 1);
        assert_eq!(state.workspaces[0].repositories[0].repository_id, 1);

        let mut unconfigured = state.clone();
        unconfigured.contexts[0].execution_machine_id = None;
        let error = decide(
            unconfigured,
            Event::CreateWorktree {
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                path: "/tmp/worktrees/mission-manager".into(),
                branch: "feature/contract".into(),
                base_branch: "main".into(),
                is_dirty: false,
            },
        )
        .expect_err("Worktree creation must be blocked until Context Machine setup");
        assert!(matches!(
            error,
            DomainError::ContextHasNoExecutionMachine { .. }
        ));

        let mut alternate_machine = state.clone();
        alternate_machine.machines.push(Machine {
            id: 2,
            context_id: 1,
            name: "Alternate".into(),
            socket_name: "alternate".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        let error = decide(
            alternate_machine,
            Event::CreateWorktree {
                workspace_id: 1,
                repository_id: 1,
                machine_id: 2,
                path: "/tmp/worktrees/mission-manager".into(),
                branch: "feature/contract".into(),
                base_branch: "main".into(),
                is_dirty: false,
            },
        )
        .expect_err("Worktree creation cannot override the Context Machine");
        assert!(matches!(
            error,
            DomainError::ContextExecutionMachineMismatch { .. }
        ));

        let worktree = decide(
            state,
            Event::CreateWorktree {
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                path: "/tmp/worktrees/mission-manager".into(),
                branch: "feature/contract".into(),
                base_branch: "main".into(),
                is_dirty: false,
            },
        )
        .expect("Worktree creation should succeed");
        assert_eq!(worktree.state.worktrees[0].workspace_id, 1);
        assert_eq!(
            worktree.state.workspaces[0].preparation_state,
            WorkspacePreparationState::Ready
        );
    }

    #[test]
    fn run_suggestions_only_attach_to_registered_workspace_locations() {
        let state = DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 1,
            next_machine_id: 3,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                execution_machine_id: Some(1),
                grill_defaults: GrillConfiguration::default(),
                implement_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "repo".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![
                RepositoryLocation {
                    repository_id: 1,
                    machine_id: 1,
                    checkout_path: "/tmp/checkouts/repo".into(),
                    worktree_root: "/tmp/worktrees".into(),
                },
                RepositoryLocation {
                    repository_id: 1,
                    machine_id: 2,
                    checkout_path: "/tmp/checkouts/repo".into(),
                    worktree_root: "/tmp/worktrees".into(),
                },
            ],
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Attach".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "main".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Pending,
            }],
            worktrees: Vec::new(),
            machines: vec![
                Machine {
                    id: 1,
                    context_id: 1,
                    name: "Local".into(),
                    socket_name: "mission".into(),
                    transport: MachineTransport::Local,
                    last_observed: MachineObservation::Available,
                    last_observed_at: None,
                },
                Machine {
                    id: 2,
                    context_id: 1,
                    name: "Other".into(),
                    socket_name: "other".into(),
                    transport: MachineTransport::Local,
                    last_observed: MachineObservation::Available,
                    last_observed_at: None,
                },
            ],
            runs: Vec::new(),
            implementation_queues: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let suggestions = suggest_untracked_runs(
            &state,
            &[
                AgentPaneObservation {
                    machine_id: 1,
                    agent: AgentKind::Codex,
                    session_name: "mission".into(),
                    pane_id: "%1".into(),
                    current_path: "/tmp/checkouts/repo/src".into(),
                    machine_home: "/Users/tester".into(),
                },
                AgentPaneObservation {
                    machine_id: 2,
                    agent: AgentKind::Codex,
                    session_name: "other".into(),
                    pane_id: "%2".into(),
                    current_path: "/tmp/checkouts/repo/src".into(),
                    machine_home: "/Users/tester".into(),
                },
            ],
        );
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].workspace_id, Some(1));
        assert_eq!(suggestions[0].repository_id, Some(1));
        assert_eq!(
            suggestions[0].location_path.as_deref(),
            Some("/tmp/checkouts/repo")
        );
    }

    #[test]
    fn home_relative_paths_do_not_match_unrelated_absolute_path_components() {
        let home = "/Users/tester";
        assert!(path_is_within("~/src/repo", "~/src/repo/service", home));
        assert!(path_is_within(
            "~/src/repo",
            &format!("{home}/src/repo/service"),
            home
        ));
        assert!(!path_is_within("~/src/repo", "/tmp/src/repo/service", home));
        assert!(!path_is_within(
            "/tmp/src/repo",
            "/tmp/src/repository/service",
            home
        ));
    }
}

#[cfg(test)]
mod grill_contract_tests {
    use super::*;

    fn state() -> DomainState {
        DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                execution_machine_id: Some(1),
                grill_defaults: GrillConfiguration::default(),
                implement_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![RepositoryLocation {
                repository_id: 1,
                machine_id: 1,
                checkout_path: "/tmp/mission-manager".into(),
                worktree_root: "/tmp/worktrees".into(),
            }],
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Decide the next architecture".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: "The decision must stay reversible.".into(),
                reminders: Vec::new(),
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "main".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Ready,
            }],
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            implementation_queues: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }

    fn start_event(configuration: GrillConfiguration) -> Event {
        let prompt = compose_grill_prompt(
            &state(),
            1,
            &configuration,
            "Stress-test this architecture decision.",
        )
        .expect("the Grill prompt should be composable");
        Event::StartGrillRun {
            item_id: 1,
            workspace_id: 1,
            repository_id: 1,
            machine_id: 1,
            configuration,
            prompt,
            skill_snapshot: grill_skill_snapshot().into(),
            working_directory: "/tmp/mission-manager".into(),
            session_name: "mission-item-1-run-1".into(),
            pane_id: "%1".into(),
            started_at: 123,
            checkouts: vec![RunCheckout {
                repository_id: 1,
                path: "/tmp/mission-manager".into(),
                branch: "main".into(),
                is_dirty: false,
            }],
        }
    }

    #[test]
    fn grill_catalog_rejects_an_effort_not_supported_by_the_selected_model() {
        let configuration = GrillConfiguration {
            agent: AgentKind::Codex,
            model: "gpt-6-luna".into(),
            effort: "not-supported".into(),
        };

        assert!(validate_grill_configuration(&configuration).is_err());
        assert!(grill_model_catalog()
            .iter()
            .any(|catalog| catalog.agent == AgentKind::Codex
                && catalog.models.iter().any(|model| {
                    model.id == "gpt-6-luna"
                        && model.efforts.iter().any(|effort| effort.id == "xhigh")
                })));
    }

    #[test]
    fn a_grill_run_persists_its_configuration_snapshot_and_primary_checkout() {
        let configuration = GrillConfiguration {
            agent: AgentKind::Codex,
            model: "gpt-6-luna".into(),
            effort: "xhigh".into(),
        };

        let decision = decide(state(), start_event(configuration.clone()))
            .expect("a valid Grill should start");
        let run = decision.state.runs.last().expect("Run should be recorded");

        assert_eq!(run.execution_profile, ExecutionProfile::Grill);
        assert_eq!(run.repository_id, Some(1));
        assert_eq!(run.working_directory, "/tmp/mission-manager");
        assert_eq!(run.model.as_deref(), Some("gpt-6-luna"));
        assert_eq!(run.effort.as_deref(), Some("xhigh"));
        assert_eq!(run.skill_snapshot.as_deref(), Some(grill_skill_snapshot()));
        assert!(run
            .prompt
            .contains("Stress-test this architecture decision."));
        assert!(run.prompt.contains("Decide the next architecture"));
        assert!(run.prompt.contains("The decision must stay reversible."));
        assert!(run.prompt.contains(grill_skill_snapshot()));
    }

    #[test]
    fn agent_state_reports_only_apply_strictly_newer_sequences() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let reports = [
            (1, RunState::Working),
            (3, RunState::Finished),
            (2, RunState::Blocked),
            (3, RunState::Blocked),
            (4, RunState::Finished),
        ];
        let mut state = started.state;
        let mut state_transitions = 0;

        for (sequence, report_state) in reports {
            let before = state.runs[0].state;
            let decision = decide(
                state,
                Event::ApplyAgentStateReport {
                    run_id: 1,
                    state: report_state,
                    sequence: Some(sequence),
                },
            )
            .expect("a report for the existing Run should be considered");
            if before != decision.state.runs[0].state {
                state_transitions += 1;
            }
            if sequence == 2 || sequence == 3 && report_state == RunState::Blocked {
                assert!(decision.effects.is_empty(), "stale reports have no effects");
            }
            state = decision.state;
        }

        assert_eq!(state.runs[0].state, RunState::Finished);
        assert_eq!(state.runs[0].last_applied_agent_state_sequence, Some(4));
        assert_eq!(state_transitions, 2);
    }

    #[test]
    fn legacy_agent_state_report_only_applies_before_versioned_history() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let legacy = decide(
            started.state,
            Event::ApplyAgentStateReport {
                run_id: 1,
                state: RunState::Working,
                sequence: None,
            },
        )
        .expect("a legacy report should apply without versioned history");
        assert_eq!(legacy.state.runs[0].state, RunState::Working);
        assert_eq!(legacy.state.runs[0].last_applied_agent_state_sequence, None);

        let versioned = decide(
            legacy.state,
            Event::ApplyAgentStateReport {
                run_id: 1,
                state: RunState::Blocked,
                sequence: Some(1),
            },
        )
        .expect("a versioned report should apply");
        let late_legacy = decide(
            versioned.state,
            Event::ApplyAgentStateReport {
                run_id: 1,
                state: RunState::Finished,
                sequence: None,
            },
        )
        .expect("a late legacy report should be ignored");

        assert_eq!(late_legacy.state.runs[0].state, RunState::Blocked);
        assert_eq!(
            late_legacy.state.runs[0].last_applied_agent_state_sequence,
            Some(1)
        );
        assert!(late_legacy.effects.is_empty());
    }

    #[test]
    fn an_item_cannot_start_a_second_active_grill_run() {
        let configuration = GrillConfiguration::default();
        let first = decide(state(), start_event(configuration.clone()))
            .expect("the first Grill should start");
        let error = decide(first.state, start_event(configuration))
            .expect_err("a second active Grill must be rejected");

        assert!(matches!(
            error,
            DomainError::ActiveGrillRun { item_id: 1, .. }
        ));
    }

    #[test]
    fn parser_extracts_recommendations_options_and_free_form_questions() {
        let group = parse_grill_question_group(
            "before the group\n\n❓ **Q1** - **Repository layout**: Which layout should we keep?\n➡️ **Keep the current layout**\nA) Keep current\nB. Split repositories\n---\n❓ 2. What should we document next?\n",
        )
        .expect("the question group should parse");

        assert_eq!(group.questions.len(), 2);
        assert_eq!(group.questions[0].number, 1);
        assert_eq!(
            group.questions[0].title.as_deref(),
            Some("Repository layout")
        );
        assert_eq!(
            group.questions[0].recommendation.as_deref(),
            Some("Keep the current layout")
        );
        assert_eq!(
            group.questions[0].options,
            vec![
                GrillOption {
                    key: "A".into(),
                    label: "Keep current".into(),
                },
                GrillOption {
                    key: "B".into(),
                    label: "Split repositories".into(),
                },
            ]
        );
        assert_eq!(group.questions[1].number, 2);
        assert!(group.questions[1].options.is_empty());
        assert_eq!(group.questions[1].prompt, "What should we document next?");
    }

    #[test]
    fn a_round_reprinted_after_a_sub_agent_replaces_the_first_one() {
        let transcript = "\
• Started `/root/inspect_devtools`

• ## Rodada 1 — decisões que já posso perguntar

  ❓ Q1 — Onde a variável deve funcionar? Você quer que ela abra o DevTools
  apenas no desenvolvimento local?

  ➡️ Recomendo limitar ao desenvolvimento local.

  ———

  ❓ Q2 — Como a variável deve ativar o DevTools? Deve abrir quando estiver
  definida com o valor true?

  ➡️ Recomendo exigir exatamente true, para valores como false não ativarem o
  DevTools por engano.

  ———

  Estou conferindo os scripts e o ponto de abertura do DevTools.

• Waiting for agents

• Finished waiting
  └ No agents completed yet

• Completed `/root/inspect_devtools`

• ## Rodada 1

  ❓ Q1 — Em quais ambientes a variável deve abrir o DevTools? O DevTools já
  está restrito a builds de debug.

  ➡️ Recomendo limitar ao desenvolvimento local.

  ———

  ❓ Q2 — Que valor deve ativar a variável? Deve abrir o DevTools somente quando
  a variável tiver o valor true, ou sempre que ela estiver definida?

  ➡️ Recomendo exigir exatamente true, para valores como false não ativarem o
  DevTools por engano.

  ———

  A causa está em src-tauri/src/lib.rs:66. Aguardo suas respostas.

  Worked for 1m 26s · done 2:40 PM
";

        let group = parse_grill_question_group(transcript).expect("the round should parse");

        assert_eq!(group.questions.len(), 2);
        assert_eq!(group.questions[0].number, 1);
        assert_eq!(
            group.questions[0].title.as_deref(),
            Some("Em quais ambientes a variável deve abrir o DevTools?")
        );
        assert_eq!(
            group.questions[0].prompt,
            "O DevTools já está restrito a builds de debug."
        );
        assert_eq!(group.questions[1].number, 2);
        assert_eq!(
            group.questions[1].prompt,
            "Deve abrir o DevTools somente quando a variável tiver o valor true, ou sempre que ela estiver definida?"
        );
        assert_eq!(
            group.questions[1].recommendation.as_deref(),
            Some("Recomendo exigir exatamente true, para valores como false não ativarem o DevTools por engano.")
        );
    }

    #[test]
    fn continuing_question_numbers_stay_in_the_same_round() {
        let group = parse_grill_question_group(
            "❓ Q1: First?\n➡️ A\n---\n❓ Q2: Second?\n➡️ B\n---\n❓ Q3: Third?\n➡️ C",
        )
        .expect("the round should parse");

        assert_eq!(
            group.questions.iter().map(|q| q.number).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn malformed_grill_text_does_not_become_a_question_group() {
        assert!(parse_grill_question_group("❓\n➡️\n---").is_none());
        assert!(parse_grill_question_group("The agent is still working").is_none());
    }

    #[test]
    fn parsing_a_later_grill_response_ignores_questions_from_the_retained_scrollback() {
        let first_transcript = "❓ Q1: Which layout should we keep?\nA) Current\n";
        let transcript =
            format!("{first_transcript}1. Current\n\n❓ Q1: Confirm the specification?\n➡️ Yes\n");

        let group = parse_grill_question_group_since(first_transcript, &transcript)
            .expect("the later response should contain a question group");

        assert_eq!(group.questions.len(), 1);
        assert_eq!(group.questions[0].prompt, "Confirm the specification?");
    }

    #[test]
    fn an_unparseable_later_grill_response_clears_the_previous_pending_group() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let first_group = parse_grill_question_group("❓ Q1: Which layout should we keep?")
            .expect("the first response should parse");
        let waiting = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "first response".into(),
                question_group: Some(first_group),
            },
        )
        .expect("the first response should be retained");
        let pane_lost = decide(
            waiting.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("a missing Pane should preserve the waiting Run");
        let reconnected = decide(
            pane_lost.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Available,
            },
        )
        .expect("the exact Pane should be recoverable");
        let answered = decide(
            reconnected.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep current".into(),
                }],
            },
        )
        .expect("the first answer should be retained");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep current".into(),
            },
        )
        .expect("the first response should be retained");

        let recaptured = decide(
            responded.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "a later response with no parseable question markers".into(),
                question_group: None,
            },
        )
        .expect("an unparseable response should still be retained");
        let run = &recaptured.state.runs[0];

        assert!(run.grill_question_group.is_none());
        assert!(run.grill_answers.is_empty());
        assert!(run.grill_response.is_none());
        assert_eq!(
            run.transcript,
            "a later response with no parseable question markers"
        );
    }

    #[test]
    fn one_grill_answer_event_persists_a_stable_numbered_response() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let group = parse_grill_question_group(
            "❓ Q1: Which option?\n➡️ Use A\nA) Use A\nB) Use B\n❓ Q2: Explain the tradeoff",
        )
        .expect("the group should parse");
        let recorded = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "raw response".into(),
                question_group: Some(group),
            },
        )
        .expect("the transcript should be recorded");
        let answered = decide(
            recorded.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![
                    GrillAnswer {
                        question_number: 2,
                        answer: "Document the tradeoff".into(),
                    },
                    GrillAnswer {
                        question_number: 1,
                        answer: "Recommendation: Use A".into(),
                    },
                ],
            },
        )
        .expect("the answers should be recorded");
        let response = format_grill_response(&answered.state.runs[0].grill_answers)
            .expect("the grouped response should format");
        let completed = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response,
            },
        )
        .expect("the grouped response should be recorded");

        assert_eq!(completed.state.runs[0].transcript, "raw response");
        assert_eq!(
            completed.state.runs[0].grill_response.as_deref(),
            Some("1. Recommendation: Use A\n2. Document the tradeoff")
        );
    }

    #[test]
    fn a_grill_run_reconciles_hook_state_and_pane_recovery_without_text_inference() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        assert_eq!(
            started.state.runs[0].grill_phase,
            Some(GrillPhase::Starting)
        );

        let working = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Working,
            },
        )
        .expect("the hook state should be recorded");
        assert_eq!(working.state.runs[0].grill_phase, Some(GrillPhase::Working));

        let group =
            parse_grill_question_group("❓ Q1: Which option?").expect("the group should parse");
        let transcript = decide(
            working.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "agent finished a response".into(),
                question_group: Some(group.clone()),
            },
        )
        .expect("transcript capture should be recorded");
        assert_eq!(
            transcript.state.runs[0].grill_phase,
            Some(GrillPhase::Working)
        );

        let answered = decide(
            transcript.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep it".into(),
                }],
            },
        )
        .expect("the answer should be recorded");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep it".into(),
            },
        )
        .expect("the response should be recorded");

        let recaptured = decide(
            responded.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "latest transcript with the same question".into(),
                question_group: Some(group),
            },
        )
        .expect("a later capture should be recorded");
        assert_eq!(recaptured.state.runs[0].grill_answers.len(), 1);
        assert_eq!(
            recaptured.state.runs[0].grill_response.as_deref(),
            Some("1. Keep it")
        );

        let blocked = decide(
            recaptured.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("the hook should report a waiting state");
        assert_eq!(
            blocked.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );

        let missing = decide(
            blocked.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("Pane loss should remain recoverable");
        let missing_run = &missing.state.runs[0];
        assert_eq!(missing_run.state, RunState::Blocked);
        assert_eq!(missing_run.pane_status, RunPaneStatus::Missing);
        assert_eq!(
            missing_run.grill_phase,
            Some(GrillPhase::RecoverablePaneLoss)
        );
        assert_eq!(missing_run.grill_answers.len(), 1);
        assert_eq!(
            missing_run.transcript,
            "latest transcript with the same question"
        );

        let reconnected = decide(
            missing.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Available,
            },
        )
        .expect("Pane recovery should restore the specialized phase");
        assert_eq!(
            reconnected.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );
        assert_eq!(reconnected.state.items[0].status, ItemStatus::Active);
    }

    #[test]
    fn unknown_pane_observation_does_not_trigger_grill_pane_loss() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let working = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Working,
            },
        )
        .expect("the hook should report the active Run");
        let previous_state = working.state.runs[0].state;

        let unknown = decide(
            working.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Unknown,
            },
        )
        .expect("an inconclusive observation should be recorded");

        assert_eq!(unknown.state.runs[0].state, previous_state);
        assert_eq!(unknown.state.runs[0].pane_status, RunPaneStatus::Unknown);
        assert_eq!(unknown.state.runs[0].grill_phase, Some(GrillPhase::Working));
    }

    #[test]
    fn finishing_a_grill_run_is_explicit_and_keeps_the_item_status_unchanged() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let finished = decide(started.state, Event::FinishRun { run_id: 1 })
            .expect("the user should be able to finish the Run explicitly");
        assert_eq!(finished.state.runs[0].state, RunState::Finished);
        assert_eq!(
            finished.state.runs[0].grill_phase,
            Some(GrillPhase::Finished)
        );
        assert_eq!(finished.state.items[0].status, ItemStatus::Active);
    }

    #[test]
    fn a_completed_grill_stays_active_until_the_user_finishes_it() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the hook completion should enter the downstream phase");

        assert_eq!(awaiting.state.runs[0].state, RunState::Finished);
        assert_eq!(
            awaiting.state.runs[0].grill_phase,
            Some(GrillPhase::AwaitingNextAction)
        );
        let error = decide(awaiting.state, start_event(GrillConfiguration::default()))
            .expect_err("an awaiting Grill still occupies the Item");
        assert!(matches!(
            error,
            DomainError::ActiveGrillRun { item_id: 1, .. }
        ));
    }

    #[test]
    fn downstream_prompt_contains_the_selected_skill_item_context_and_decisions() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let transcript = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "❓ Q1: Which decision should we keep?".into(),
                question_group: parse_grill_question_group("❓ Q1: Which decision should we keep?"),
            },
        )
        .expect("the Grill transcript should be recorded");
        let answered = decide(
            transcript.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep the reversible design".into(),
                }],
            },
        )
        .expect("the decision should be recorded");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep the reversible design".into(),
            },
        )
        .expect("the grouped response should be recorded");
        let awaiting = decide(
            responded.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should await a downstream action");

        for action in [
            GrillContinuationAction::ToSpec,
            GrillContinuationAction::ToTickets,
            GrillContinuationAction::Implement,
        ] {
            let prompt = compose_grill_continuation_prompt(&awaiting.state, 1, action)
                .expect("the downstream prompt should compose");
            assert!(prompt.contains(action.skill_snapshot()));
            assert!(prompt.contains("Decide the next architecture"));
            assert!(prompt.contains("The decision must stay reversible."));
            assert!(
                !prompt.contains("❓ Q1: Which decision should we keep?"),
                "the Pane transcript is already in the agent's context"
            );
            assert!(prompt.contains("Q1: Keep the reversible design"));
            assert!(prompt.contains("docs/agents/issue-tracker.md"));
            assert!(prompt.contains(GRILL_OUTPUT_CONTRACT));
            assert!(prompt.contains(action.as_str()));
            assert!(prompt.contains("same Run and Pane"));
            assert!(prompt.contains("AI_MISSION_MANAGER_EVENT"));
            assert!(prompt.contains("github.issue.created"));
            assert!(prompt.contains("\"run_id\":1"));
        }
    }

    #[test]
    fn selecting_a_downstream_skill_reuses_the_run_and_allows_grouped_questions_again() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should await a downstream action");
        let continued = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                started_at: 100,
            },
        )
        .expect("to-spec should continue the existing Run");
        assert_eq!(continued.state.runs[0].id, 1);
        assert_eq!(continued.state.runs[0].state, RunState::Working);
        assert_eq!(
            continued.state.runs[0].grill_phase,
            Some(GrillPhase::Working)
        );
        assert_eq!(continued.state.runs[0].pane_id, "%1");
        assert_eq!(
            continued.state.runs[0].working_directory,
            "/tmp/mission-manager"
        );
        assert!(continued.state.runs[0].grill_question_group.is_none());
        assert!(continued.state.runs[0].grill_response.is_none());

        let group = parse_grill_question_group("❓ Q1: Confirm the specification?")
            .expect("the downstream skill should use the same parser");
        let waiting = decide(
            continued.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("the downstream hook should report a question");
        let recorded = decide(
            waiting.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "downstream confirmation".into(),
                question_group: Some(group),
            },
        )
        .expect("the downstream question group should be recorded");
        assert_eq!(
            recorded.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );
        assert_eq!(recorded.state.runs[0].session_name, "mission-item-1-run-1");
    }

    #[test]
    fn the_work_projection_keeps_one_grill_run_through_answers_recovery_and_both_downstream_skills()
    {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let working = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Working,
            },
        )
        .expect("the hook should report the active Grill");
        let first_group = parse_grill_question_group("❓ Q1: Which decision should we keep?")
            .expect("the first question group should parse");
        let transcript = decide(
            working.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "initial Grill response\n❓ Q1: Which decision should we keep?".into(),
                question_group: Some(first_group),
            },
        )
        .expect("the initial transcript should be retained");
        let waiting = decide(
            transcript.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("the hook should put the Run into the waiting phase");
        let answered = decide(
            waiting.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep the reversible design".into(),
                }],
            },
        )
        .expect("the grouped answer should be persisted");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep the reversible design".into(),
            },
        )
        .expect("the grouped answer should be recorded as a response");
        let grill_finished = decide(
            responded.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the completed Grill should await a downstream action");
        assert_eq!(
            grill_finished.state.runs[0].grill_phase,
            Some(GrillPhase::AwaitingNextAction)
        );

        let to_spec = decide(
            grill_finished.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                started_at: 100,
            },
        )
        .expect("to-spec should reuse the Run");
        let spec_finished = decide(
            to_spec.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("to-spec should return to the downstream action frontier");
        let spec_issue = confirmed_downstream_issue("https://github.com/acme/app/issues/7");
        let spec_captured = decide(
            spec_finished.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                issues: vec![spec_issue],
            },
        )
        .expect("to-spec output should link its confirmed Issue");

        let to_tickets = decide(
            spec_captured.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                started_at: 100,
            },
        )
        .expect("to-tickets should reuse the same Run");
        let tickets_finished = decide(
            to_tickets.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("to-tickets should return to the downstream action frontier");
        let tickets_issue = confirmed_downstream_issue("https://github.com/acme/app/issues/8");
        let captured = decide(
            tickets_finished.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![tickets_issue],
            },
        )
        .expect("to-tickets output should link its confirmed Issue");

        let view = home_view(&captured.state, None, "2026-09-22T12:00");
        let item = view
            .running
            .iter()
            .find(|item| item.item.id == 1)
            .expect("the Item should remain in its original Work column");
        assert_eq!(item.item.status, ItemStatus::Active);
        assert_eq!(item.runs.len(), 1);
        assert_eq!(item.runs[0].id, 1);
        assert_eq!(item.runs[0].pane_id, "%1");
        assert_eq!(item.runs[0].working_directory, "/tmp/mission-manager");
        assert_eq!(item.links.len(), 2);
        assert_eq!(
            item.links
                .iter()
                .map(|link| link
                    .link
                    .provenance
                    .as_ref()
                    .map(|provenance| provenance.action))
                .collect::<Vec<_>>(),
            vec![
                Some(GrillContinuationAction::ToSpec),
                Some(GrillContinuationAction::ToTickets)
            ]
        );

        let finished = decide(captured.state, Event::FinishRun { run_id: 1 })
            .expect("finishing the Run should be explicit");
        assert_eq!(finished.state.items[0].status, ItemStatus::Active);
        assert_eq!(
            finished.state.runs[0].grill_phase,
            Some(GrillPhase::Finished)
        );
    }

    #[test]
    fn downstream_action_requires_the_completed_grill_and_an_available_pane() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let error = decide(
            started.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::Implement,
                started_at: 100,
            },
        )
        .expect_err("a Grill still asking questions cannot jump downstream");
        assert!(matches!(
            error,
            DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
        ));

        let awaiting = decide(
            decide(state(), start_event(GrillConfiguration::default()))
                .expect("the Grill should start")
                .state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should await a downstream action");
        let missing = decide(
            awaiting.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("Pane loss should be recoverable");
        let error = decide(
            missing.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::Implement,
                started_at: 100,
            },
        )
        .expect_err("a missing Pane cannot receive a continuation");
        assert!(matches!(
            error,
            DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
        ));
    }

    #[test]
    fn downstream_issue_discovery_keeps_structured_provenance_and_plain_issue_urls() {
        let structured = discover_downstream_issue_candidates(
            r#"AI_MISSION_MANAGER_EVENT {"event":"github.issue.created","url":"https://github.com/acme/app/issues/7","run_id":1,"action":"to-tickets"}
https://github.com/acme/app/issues/8
https://example.com/unrelated"#,
        );
        assert_eq!(structured.len(), 2);
        assert_eq!(structured[0].url, "https://github.com/acme/app/issues/7");
        assert_eq!(
            structured[0].discovery,
            DownstreamIssueDiscovery::StructuredEvent
        );
        assert_eq!(structured[0].run_id, Some(1));
        assert_eq!(
            structured[0].action,
            Some(GrillContinuationAction::ToTickets)
        );
        assert_eq!(structured[1].url, "https://github.com/acme/app/issues/8");
        assert_eq!(structured[1].discovery, DownstreamIssueDiscovery::OutputUrl);

        let fallback = discover_downstream_issue_candidates(
            "Created issues: https://github.com/acme/app/issues/8/ and https://github.com/acme/app/issues/8/).",
        );
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].url, "https://github.com/acme/app/issues/8");
        assert_eq!(fallback[0].discovery, DownstreamIssueDiscovery::OutputUrl);
    }

    fn waiting_for_answers_state() -> DomainState {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let transcript = "❓ Q1 - **Storage**: Where do drafts live?\n➡️ Keep them local";
        let recorded = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: transcript.into(),
                question_group: parse_grill_question_group(transcript),
            },
        )
        .expect("the question group should be recorded");
        let waiting = decide(
            recorded.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should wait for answers");
        assert_eq!(
            waiting.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );
        waiting.state
    }

    #[test]
    fn to_spec_can_cut_a_grill_short_while_it_waits_for_answers() {
        let waiting = waiting_for_answers_state();

        let prompt =
            compose_grill_continuation_prompt(&waiting, 1, GrillContinuationAction::ToSpec)
                .expect("the early to-spec prompt should compose");
        assert!(prompt.contains("stopped the Grill early"));
        assert!(prompt.contains("Q1: Storage Where do drafts live? (recommended: Keep them local)"));

        let continued = decide(
            waiting,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                started_at: 500,
            },
        )
        .expect("to-spec should continue a Grill that waits for answers");
        let run = &continued.state.runs[0];
        assert_eq!(run.grill_phase, Some(GrillPhase::Working));
        assert_eq!(run.grill_action, Some(GrillContinuationAction::ToSpec));
        assert_eq!(run.grill_action_started_at, Some(500));
        assert!(run.grill_question_group.is_none());
    }

    #[test]
    fn only_to_spec_can_start_before_the_grill_is_complete() {
        for action in [
            GrillContinuationAction::ToTickets,
            GrillContinuationAction::Implement,
        ] {
            let error = decide(
                waiting_for_answers_state(),
                Event::ContinueGrill {
                    run_id: 1,
                    action,
                    started_at: 500,
                },
            )
            .expect_err("only to-spec may cut the Grill short");
            assert!(matches!(
                error,
                DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
            ));
        }
    }

    /// A Grill whose to-spec step asked its own confirmation questions.
    fn to_spec_waiting_for_answers_state() -> DomainState {
        let continued = decide(
            waiting_for_answers_state(),
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                started_at: 500,
            },
        )
        .expect("to-spec should continue the Grill");
        let transcript = "❓ Q1 - **Next step**: Close the spec step?\n➡️ Close it";
        let recorded = decide(
            continued.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: transcript.into(),
                question_group: parse_grill_question_group(transcript),
            },
        )
        .expect("the to-spec questions should be recorded");
        let waiting = decide(
            recorded.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("to-spec should wait for answers");
        assert_eq!(
            waiting.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );
        waiting.state
    }

    #[test]
    fn to_tickets_can_skip_the_questions_of_to_spec() {
        let waiting = to_spec_waiting_for_answers_state();

        let prompt =
            compose_grill_continuation_prompt(&waiting, 1, GrillContinuationAction::ToTickets)
                .expect("the to-tickets prompt should compose");
        assert!(!prompt.contains("stopped the Grill early"));
        assert!(prompt.contains("moved on from to-spec"));
        assert!(prompt.contains("Q1: Next step Close the spec step? (recommended: Close it)"));

        let continued = decide(
            waiting,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                started_at: 600,
            },
        )
        .expect("to-tickets should follow a to-spec that waits for answers");
        assert_eq!(
            continued.state.runs[0].grill_action,
            Some(GrillContinuationAction::ToTickets)
        );
    }

    #[test]
    fn only_the_next_action_can_skip_the_questions_of_a_downstream_action() {
        for action in [
            GrillContinuationAction::ToSpec,
            GrillContinuationAction::Implement,
        ] {
            let error = decide(
                to_spec_waiting_for_answers_state(),
                Event::ContinueGrill {
                    run_id: 1,
                    action,
                    started_at: 600,
                },
            )
            .expect_err("only to-tickets follows to-spec");
            assert!(matches!(
                error,
                DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
            ));
        }
    }

    #[test]
    fn to_tickets_receives_the_spec_created_earlier_in_the_run() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should finish");
        let spec = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                started_at: 100,
            },
        )
        .expect("to-spec should continue the Run");
        let captured = decide(
            spec.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                issues: vec![confirmed_downstream_issue(
                    "https://github.com/acme/app/issues/9",
                )],
            },
        )
        .expect("the spec Issue should be captured");

        let prompt = compose_grill_continuation_prompt(
            &captured.state,
            1,
            GrillContinuationAction::ToTickets,
        )
        .expect("the to-tickets prompt should compose");
        assert!(prompt.contains("Spec created earlier in this Run"));
        assert!(prompt.contains("https://github.com/acme/app/issues/9"));
    }

    #[test]
    fn only_issues_created_after_the_action_started_are_downstream_output() {
        let snapshot = |created: Option<&str>| ExternalSnapshotData {
            title: "Issue".into(),
            state: "OPEN".into(),
            metadata: created
                .map(|value| ExternalMetadata {
                    key: "created".into(),
                    value: value.into(),
                })
                .into_iter()
                .collect(),
            fetched_at: 0,
        };
        let started_at = parse_github_timestamp("2026-09-25T12:00:00Z");
        assert_eq!(started_at, Some(1_790_337_600));
        assert_eq!(
            parse_github_timestamp("2024-02-29T23:59:59Z"),
            Some(1_709_251_199)
        );

        assert!(downstream_issue_is_new(
            &snapshot(Some("2026-09-25T12:03:00Z")),
            started_at
        ));
        assert!(
            downstream_issue_is_new(&snapshot(Some("2026-09-25T11:59:00Z")), started_at),
            "a small clock skew is tolerated"
        );
        assert!(!downstream_issue_is_new(
            &snapshot(Some("2026-09-01T09:00:00Z")),
            started_at
        ));
        assert!(!downstream_issue_is_new(&snapshot(None), started_at));
        assert!(downstream_issue_is_new(&snapshot(None), None));
    }

    #[test]
    fn captured_downstream_issue_is_idempotent_and_keeps_run_action_provenance() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should finish");
        let continued = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                started_at: 100,
            },
        )
        .expect("to-tickets should continue the Run");
        let issue = confirmed_downstream_issue("https://github.com/acme/app/issues/7");
        let second_item = decide(
            continued.state,
            Event::CreateItem {
                title: "Another Item".into(),
                context_id: 1,
                project_id: 1,
            },
        )
        .expect("a second Item should be created");
        let linked_elsewhere = decide(
            second_item.state,
            Event::LinkExternalObject {
                item_id: 2,
                object: issue.object.clone(),
                snapshot: Some(issue.snapshot.clone()),
            },
        )
        .expect("the same External Object should be linkable to another Item");

        let first = decide(
            linked_elsewhere.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![issue.clone()],
            },
        )
        .expect("the confirmed Issue should be captured");
        assert_eq!(first.state.external_objects.len(), 1);
        assert_eq!(first.state.links.len(), 2);
        assert_eq!(
            first
                .state
                .links
                .iter()
                .find(|link| link.item_id == 1)
                .expect("the captured Item Link should exist")
                .provenance,
            Some(LinkProvenance {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                discovery: DownstreamIssueDiscovery::StructuredEvent,
            })
        );

        let second = decide(
            first.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![issue],
            },
        )
        .expect("capturing the same Issue again should be safe");
        assert_eq!(second.state.external_objects.len(), 1);
        assert_eq!(second.state.links.len(), 2);
        assert!(second.effects.is_empty());
    }

    #[test]
    fn deleting_a_captured_link_only_removes_local_external_state() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should finish");
        let continued = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                started_at: 100,
            },
        )
        .expect("to-tickets should continue the Run");
        let captured = decide(
            continued.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![confirmed_downstream_issue(
                    "https://github.com/acme/app/issues/7",
                )],
            },
        )
        .expect("the Issue should be captured");

        let deleted = decide(captured.state, Event::DeleteLink { link_id: 1 })
            .expect("deleting the local Link should succeed");
        assert!(deleted.state.links.is_empty());
        assert!(deleted.state.external_objects.is_empty());
        assert!(deleted.effects.iter().all(|effect| matches!(
            effect,
            Effect::RemoveLink { .. } | Effect::RemoveExternalObject { .. }
        )));
    }

    fn confirmed_downstream_issue(url: &str) -> ConfirmedDownstreamIssue {
        let parts = url.trim_end_matches('/').split('/').collect::<Vec<_>>();
        let external_key = format!(
            "issue:{}/{}#{}",
            parts[3].to_ascii_lowercase(),
            parts[4].to_ascii_lowercase(),
            parts[6]
        );
        let object = ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind: ExternalObjectKind::Issue,
            external_key,
            canonical_url: url.into(),
        };
        ConfirmedDownstreamIssue {
            object,
            snapshot: ExternalSnapshotData {
                title: "Captured Issue".into(),
                state: "OPEN".into(),
                metadata: vec![],
                fetched_at: 123,
            },
            discovery: DownstreamIssueDiscovery::StructuredEvent,
        }
    }
}
