use super::*;

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
                grill_defaults: GrillConfiguration::default(),
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
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                grill_defaults: GrillConfiguration::default(),
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
            repository_locations: vec![RepositoryLocation {
                repository_id: 1,
                machine_id: 1,
                checkout_path: "/tmp/checkouts/repo".into(),
                worktree_root: "/tmp/worktrees".into(),
            }],
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
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let suggestions = suggest_untracked_runs(
            &state,
            &[AgentPaneObservation {
                machine_id: 1,
                agent: AgentKind::Codex,
                session_name: "mission".into(),
                pane_id: "%1".into(),
                current_path: "/tmp/checkouts/repo/src".into(),
                machine_home: "/Users/tester".into(),
            }],
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
                grill_defaults: GrillConfiguration::default(),
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
            skill_snapshot: GRILL_SKILL_SNAPSHOT.into(),
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
            model: "codex-luna".into(),
            effort: "not-supported".into(),
        };

        assert!(validate_grill_configuration(&configuration).is_err());
        assert!(grill_model_catalog()
            .iter()
            .any(|catalog| catalog.agent == AgentKind::Codex
                && catalog.models.iter().any(|model| {
                    model.id == "codex-luna"
                        && model.efforts.iter().any(|effort| effort.id == "xhigh")
                })));
    }

    #[test]
    fn a_grill_run_persists_its_configuration_snapshot_and_primary_checkout() {
        let configuration = GrillConfiguration {
            agent: AgentKind::Codex,
            model: "codex-luna".into(),
            effort: "xhigh".into(),
        };

        let decision = decide(state(), start_event(configuration.clone()))
            .expect("a valid Grill should start");
        let run = decision.state.runs.last().expect("Run should be recorded");

        assert_eq!(run.execution_profile, ExecutionProfile::Grill);
        assert_eq!(run.repository_id, Some(1));
        assert_eq!(run.working_directory, "/tmp/mission-manager");
        assert_eq!(run.model.as_deref(), Some("codex-luna"));
        assert_eq!(run.effort.as_deref(), Some("xhigh"));
        assert_eq!(run.skill_snapshot.as_deref(), Some(GRILL_SKILL_SNAPSHOT));
        assert!(run
            .prompt
            .contains("Stress-test this architecture decision."));
        assert!(run.prompt.contains("Decide the next architecture"));
        assert!(run.prompt.contains("The decision must stay reversible."));
        assert!(run.prompt.contains(GRILL_SKILL_SNAPSHOT));
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
    fn downstream_prompt_contains_the_selected_skill_item_context_transcript_and_decisions() {
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
            assert!(prompt.contains("❓ Q1: Which decision should we keep?"));
            assert!(prompt.contains("Q1: Keep the reversible design"));
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
            },
        )
        .expect_err("a missing Pane cannot receive a continuation");
        assert!(matches!(
            error,
            DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
        ));
    }

    #[test]
    fn downstream_issue_discovery_prefers_structured_events_and_falls_back_to_issue_urls() {
        let structured = discover_downstream_issue_candidates(
            r#"AI_MISSION_MANAGER_EVENT {"event":"github.issue.created","url":"https://github.com/acme/app/issues/7","run_id":1,"action":"to-tickets"}
https://github.com/acme/app/issues/8
https://example.com/unrelated"#,
        );
        assert_eq!(structured.len(), 1);
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

        let fallback = discover_downstream_issue_candidates(
            "Created issues: https://github.com/acme/app/issues/8/ and https://github.com/acme/app/issues/8/).",
        );
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].url, "https://github.com/acme/app/issues/8");
        assert_eq!(fallback[0].discovery, DownstreamIssueDiscovery::OutputUrl);
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
