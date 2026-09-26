use std::fs;
use std::path::Path;

use rusqlite::Connection;
use tempfile::tempdir;

use crate::domain::{
    compose_grill_prompt, decide, format_grill_response, grill_skill_snapshot,
    parse_grill_question_group, AgentKind, AuditAction, ConfirmedDownstreamIssue,
    DownstreamIssueDiscovery, Effect, Event, GrillAnswer, GrillConfiguration,
    GrillContinuationAction, GrillPhase, ImplementationQueue, ImplementationQueueEntry,
    ImplementationQueuePauseReason, LinkProvenance, MachineTransport, RunCheckout,
    WorkspaceRepositoryInput,
};

use super::{
    schema::{migrate_legacy_workset_data, table_columns},
    SqliteStore,
};

fn apply_event(
    store: &mut SqliteStore,
    state: crate::domain::DomainState,
    event: Event,
) -> crate::domain::DomainState {
    let decision = decide(state, event).expect("event should be accepted");
    store
        .apply(&decision.effects)
        .expect("event effects should persist");
    decision.state
}

#[test]
fn implementation_queue_and_first_run_link_round_trip() {
    let directory = tempdir().expect("temporary directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut store = SqliteStore::open(&database).expect("database should open");
    let state = store.load_state().expect("initial state should load");
    let project_id = state.projects[0].id;
    store.connection.execute(
        "INSERT INTO items (id, human_identifier, title, project_id, status, notes) VALUES (77, 'I-77', 'Queued item', ?1, 'Inbox', '')",
        [project_id],
    ).expect("fixture Item should be inserted");
    let queue = ImplementationQueue {
        id: 9,
        item_id: 77,
        spec_external_object_id: 3,
        spec_url: "https://github.com/o/r/issues/87".into(),
        workspace_id: 4,
        repository_id: 5,
        configuration: GrillConfiguration::default(),
        allow_dirty: false,
        allow_shared_checkouts: false,
        active: true,
        paused_reason: Some(ImplementationQueuePauseReason::CheckoutDirty),
        entries: vec![ImplementationQueueEntry {
            position: 0,
            ticket_number: 89,
            ticket_title: "First".into(),
            ticket_url: "https://github.com/o/r/issues/89".into(),
            ticket_state: "open".into(),
            run_id: Some(9),
            done: false,
            skipped: true,
        }],
    };
    store
        .apply(&[Effect::PersistImplementationQueue { queue }])
        .expect("queue should persist");
    let reloaded = SqliteStore::open(&database)
        .expect("database should reopen")
        .load_state()
        .expect("queue should reload");
    assert_eq!(reloaded.implementation_queues[0].entries[0].run_id, Some(9));
    assert!(reloaded.implementation_queues[0].entries[0].skipped);
    assert_eq!(
        reloaded.implementation_queues[0].paused_reason,
        Some(ImplementationQueuePauseReason::CheckoutDirty)
    );
}

#[test]
fn schema_upgrade_preserves_link_purpose_from_legacy_spec_and_provenance() {
    let directory = tempdir().expect("temporary directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    seed_link_purpose_migration_fixture(&database);
    let connection = Connection::open(&database).expect("database should reopen directly");
    connection
        .execute_batch(
            "ALTER TABLE external_links ADD COLUMN is_spec INTEGER NOT NULL DEFAULT 0;
             UPDATE external_links SET is_spec = 1 WHERE id = 1;
             ALTER TABLE external_links DROP COLUMN purpose;",
        )
        .expect("the previous Spec-only Link schema should be restored");
    drop(connection);

    let reloaded = SqliteStore::open(&database)
        .expect("the schema should migrate")
        .load_state()
        .expect("the migrated Links should load");
    assert_eq!(
        reloaded.links[0].purpose,
        crate::domain::LinkPurpose::ToSpec
    );
    assert_eq!(
        reloaded.links[1].purpose,
        crate::domain::LinkPurpose::ToTickets
    );
    assert_eq!(
        reloaded.links[2].purpose,
        crate::domain::LinkPurpose::Others
    );
}

#[test]
fn schema_upgrade_backfills_link_purpose_from_provenance_without_legacy_flag() {
    let directory = tempdir().expect("temporary directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    seed_link_purpose_migration_fixture(&database);
    let connection = Connection::open(&database).expect("database should reopen directly");
    connection
        .execute("ALTER TABLE external_links DROP COLUMN purpose", [])
        .expect("the original Link schema should be restored");
    drop(connection);

    let reloaded = SqliteStore::open(&database)
        .expect("the schema should migrate")
        .load_state()
        .expect("the migrated Links should load");
    assert_eq!(
        reloaded.links[0].purpose,
        crate::domain::LinkPurpose::ToSpec
    );
    assert_eq!(
        reloaded.links[1].purpose,
        crate::domain::LinkPurpose::ToTickets
    );
    assert_eq!(
        reloaded.links[2].purpose,
        crate::domain::LinkPurpose::ToSpec
    );
}

fn seed_link_purpose_migration_fixture(database: &Path) {
    let store = SqliteStore::open(&database).expect("database should open");
    store
        .connection
        .execute(
            "INSERT INTO items (id, human_identifier, title, project_id, status, notes)
             VALUES (1, 'MC-1', 'Spec migration', 1, 'Inbox', '')",
            [],
        )
        .expect("fixture Item should be inserted");
    for (id, key, number, action) in [
        (1, "acme/app#92", 92, "to-spec"),
        (2, "acme/app#93", 93, "to-tickets"),
        (3, "acme/app#94", 94, "to-spec"),
    ] {
        store
            .connection
            .execute(
                "INSERT INTO external_objects (id, provider, kind, external_key, canonical_url)
                 VALUES (?1, 'github', 'issue', ?2, ?3)",
                rusqlite::params![
                    id,
                    key,
                    format!("https://github.com/acme/app/issues/{number}")
                ],
            )
            .expect("fixture External Object should be inserted");
        store
            .connection
            .execute(
                "INSERT INTO external_links (id, item_id, external_object_id) VALUES (?1, 1, ?2)",
                rusqlite::params![id, id],
            )
            .expect("fixture Link should be inserted");
        store
            .connection
            .execute(
                "INSERT INTO link_attention_state (link_id, provenance_json)
                 VALUES (?1, ?2)",
                rusqlite::params![
                    id,
                    format!(r#"{{"run_id":18,"action":"{action}","discovery":"output-url"}}"#)
                ],
            )
            .expect("fixture Link provenance should be inserted");
    }
    drop(store);
}

#[test]
fn deleted_personal_context_stays_deleted_after_reopen() {
    let directory = tempdir().expect("temporary directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut store = SqliteStore::open(&database).expect("database should open");
    let state = store.load_state().expect("initial state should load");
    assert!(state
        .contexts
        .iter()
        .any(|context| context.name == "Personal"));
    let state = apply_event(
        &mut store,
        state,
        Event::CreateContext {
            name: "Other".into(),
        },
    );

    let decision = decide(
        state,
        Event::DeleteContext {
            context_id: 1,
            project_ids: vec![1],
            item_ids: Vec::new(),
            repository_ids: Vec::new(),
            workspace_ids: Vec::new(),
            machine_ids: Vec::new(),
        },
    )
    .expect("Personal should be deletable while another Context exists");
    store
        .apply(&decision.effects)
        .expect("context deletion should persist");
    assert!(!store
        .load_state()
        .expect("state after deletion should load")
        .contexts
        .iter()
        .any(|context| context.name == "Personal"));

    drop(store);
    let reopened = SqliteStore::open(&database).expect("database should reopen");
    assert!(!reopened
        .load_state()
        .expect("reopened state should load")
        .contexts
        .iter()
        .any(|context| context.name == "Personal"));
}

#[test]
fn context_execution_machine_round_trips() {
    let directory = tempdir().expect("temporary directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let mut store = SqliteStore::open(&database).expect("database should open");
    let state = store.load_state().expect("initial state should load");
    let state = apply_event(
        &mut store,
        state,
        Event::RegisterMachine {
            context_id: 1,
            name: "Build Mac".into(),
            socket_name: "mission".into(),
            transport: MachineTransport::Local,
        },
    );
    let _state = apply_event(
        &mut store,
        state,
        Event::SetContextExecutionMachine {
            context_id: 1,
            machine_id: Some(1),
        },
    );

    let reloaded = store.load_state().expect("updated state should load");
    assert_eq!(reloaded.contexts[0].execution_machine_id, Some(1));
    let _state = apply_event(
        &mut store,
        reloaded,
        Event::SetContextExecutionMachine {
            context_id: 1,
            machine_id: None,
        },
    );
    assert_eq!(
        store
            .load_state()
            .expect("unconfigured Context should load")
            .contexts[0]
            .execution_machine_id,
        None
    );
    drop(store);

    let reopened = SqliteStore::open(&database).expect("database should reopen");
    assert_eq!(
        reopened
            .load_state()
            .expect("unconfigured Context should remain unconfigured")
            .contexts[0]
            .execution_machine_id,
        None
    );
}

#[test]
fn machine_deletion_removes_metadata_but_leaves_checkout_and_worktree_files() {
    let directory = tempdir().expect("temporary directory should exist");
    let database = directory.path().join("mission-manager.sqlite");
    let machine_files = directory.path().join("machine-files");
    let checkout = machine_files.join("checkout");
    let worktree = machine_files.join("worktree");
    fs::create_dir_all(&checkout).expect("checkout directory should exist");
    fs::create_dir_all(&worktree).expect("worktree directory should exist");
    fs::write(checkout.join("local-change.txt"), "keep").expect("checkout file should exist");
    fs::write(worktree.join("local-change.txt"), "keep").expect("worktree file should exist");

    let mut store = SqliteStore::open(&database).expect("database should open");
    let state = store.load_state().expect("initial state should load");
    let state = apply_event(
        &mut store,
        state,
        Event::RegisterMachine {
            context_id: 1,
            name: "Build Mac".into(),
            socket_name: "mission".into(),
            transport: MachineTransport::Local,
        },
    );
    let state = apply_event(
        &mut store,
        state,
        Event::SetContextExecutionMachine {
            context_id: 1,
            machine_id: Some(1),
        },
    );
    let state = apply_event(
        &mut store,
        state,
        Event::RegisterRepositoryAtLocation {
            project_id: 1,
            name: "mission-manager".into(),
            remote_url: "https://example.test/repo".into(),
            base_branch: "main".into(),
            machine_id: 1,
            checkout_path: checkout.to_string_lossy().into_owned(),
            worktree_root: machine_files.to_string_lossy().into_owned(),
        },
    );
    let state = apply_event(
        &mut store,
        state,
        Event::CreateItem {
            title: "Preserve files when deleting a Machine".into(),
            context_id: 1,
            project_id: 1,
        },
    );
    let state = apply_event(
        &mut store,
        state,
        Event::CreateWorkspace {
            item_id: 1,
            repositories: vec![WorkspaceRepositoryInput {
                repository_id: 1,
                branch: "mission-MC-1".into(),
                base_branch: "main".into(),
            }],
        },
    );
    let state = apply_event(
        &mut store,
        state,
        Event::CreateWorktree {
            workspace_id: 1,
            repository_id: 1,
            machine_id: 1,
            path: worktree.to_string_lossy().into_owned(),
            branch: "mission-MC-1".into(),
            base_branch: "main".into(),
            is_dirty: false,
        },
    );
    let decision = decide(
        state,
        Event::DeleteMachine {
            machine_id: 1,
            run_ids: Vec::new(),
            worktree_ids: vec![1],
            repository_location_repository_ids: vec![1],
        },
    )
    .expect("Machine metadata should be deletable");
    store
        .apply(&decision.effects)
        .expect("Machine metadata should be removed");

    let reloaded = store
        .load_state()
        .expect("state after deletion should load");
    assert!(reloaded.machines.is_empty());
    assert_eq!(reloaded.contexts[0].execution_machine_id, None);
    assert!(reloaded.repository_locations.is_empty());
    assert!(reloaded.worktrees.is_empty());
    assert_eq!(
        fs::read_to_string(checkout.join("local-change.txt")).unwrap(),
        "keep"
    );
    assert_eq!(
        fs::read_to_string(worktree.join("local-change.txt")).unwrap(),
        "keep"
    );
}

fn create_legacy_schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE metadata (
                     key TEXT PRIMARY KEY NOT NULL,
                     value INTEGER NOT NULL
                 );
                 INSERT INTO metadata (key, value)
                 VALUES ('workset_migration_completed', 0);
                 CREATE TABLE worksets (
                     id INTEGER PRIMARY KEY NOT NULL,
                     item_id INTEGER NOT NULL,
                     root_directory TEXT NOT NULL,
                     branch TEXT NOT NULL,
                     archived INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE workset_repositories (
                     workset_id INTEGER NOT NULL,
                     repository_id INTEGER NOT NULL,
                     branch_override TEXT,
                     base_branch_override TEXT,
                     current_branch TEXT NOT NULL,
                     is_dirty INTEGER NOT NULL DEFAULT 0,
                     PRIMARY KEY (workset_id, repository_id)
                 );
                 CREATE TABLE external_links (
                     item_id INTEGER NOT NULL,
                     external_object_id INTEGER NOT NULL
                 );
                 CREATE TABLE activities (
                     id INTEGER PRIMARY KEY NOT NULL,
                     external_object_id INTEGER NOT NULL,
                     observed_at INTEGER NOT NULL,
                     changes_json TEXT NOT NULL
                 );
                 CREATE TABLE audit_entries (
                     id INTEGER PRIMARY KEY NOT NULL,
                     recorded_at INTEGER NOT NULL,
                     action_json TEXT NOT NULL
                 );
                 CREATE TABLE items (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE workspaces (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE repositories (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE machines (id INTEGER PRIMARY KEY NOT NULL);
                 CREATE TABLE worktrees (id INTEGER PRIMARY KEY NOT NULL);
                 INSERT INTO items (id) VALUES (1);
                 INSERT INTO machines (id) VALUES (1);
                 CREATE TABLE runs (
                     id INTEGER PRIMARY KEY NOT NULL,
                     item_id INTEGER NOT NULL,
                     workset_id INTEGER,
                     workspace_id INTEGER,
                     machine_id INTEGER NOT NULL,
                     agent TEXT NOT NULL,
                     execution_profile TEXT NOT NULL,
                     prompt TEXT NOT NULL,
                     working_directory TEXT NOT NULL,
                     session_name TEXT NOT NULL,
                     pane_id TEXT NOT NULL,
                     started_at INTEGER NOT NULL,
                     state TEXT NOT NULL DEFAULT 'unknown',
                     pane_status TEXT NOT NULL DEFAULT 'unknown'
                 );
                 CREATE INDEX runs_by_workset ON runs (workset_id, id);",
        )
        .expect("legacy schema should be created");
}

#[test]
fn migrates_actual_legacy_runs_schema_without_direct_checkouts_column() {
    let directory = tempdir().expect("temporary directory should exist");
    let legacy_workset_directory = directory.path().join("legacy-workset");
    fs::create_dir(&legacy_workset_directory).expect("legacy directory should exist");
    fs::write(legacy_workset_directory.join("keep.txt"), "keep")
        .expect("legacy directory marker should be written");

    let mut connection = Connection::open_in_memory().expect("database should open");
    create_legacy_schema(&connection);
    let legacy_workset_directory_path = legacy_workset_directory.to_string_lossy().to_string();
    connection
        .execute(
            "INSERT INTO worksets (id, item_id, root_directory, branch)
                 VALUES (1, 1, ?1, 'legacy-branch')",
            [&legacy_workset_directory_path],
        )
        .expect("legacy workset should be inserted");
    connection
        .execute(
            "INSERT INTO runs
                    (id, item_id, workset_id, workspace_id, machine_id, agent,
                     execution_profile, prompt, working_directory, session_name, pane_id,
                     started_at)
                 VALUES (1, 1, 1, NULL, 1, 'codex', 'implement', 'remove me', '/tmp',
                         'session-1', '%1', 123)",
            [],
        )
        .expect("legacy run should be inserted");
    connection
        .execute(
            "INSERT INTO runs
                    (id, item_id, workset_id, workspace_id, machine_id, agent,
                     execution_profile, prompt, working_directory, session_name, pane_id,
                     started_at)
                 VALUES (2, 1, NULL, NULL, 1, 'codex', 'implement', 'keep me', '/tmp',
                         'session-2', '%2', 456)",
            [],
        )
        .expect("unassociated legacy run should be inserted");

    migrate_legacy_workset_data(&mut connection).expect("legacy data should migrate");

    assert!(table_columns(&connection, "worksets")
        .expect("workset table lookup should succeed")
        .is_empty());
    assert!(table_columns(&connection, "workset_repositories")
        .expect("workset repository table lookup should succeed")
        .is_empty());
    assert!(table_columns(&connection, "runs")
        .expect("runs table lookup should succeed")
        .contains(&"direct_checkouts_json".to_owned()));
    let direct_checkouts: String = connection
        .query_row(
            "SELECT direct_checkouts_json FROM runs WHERE id = 2",
            [],
            |row| row.get(0),
        )
        .expect("surviving run should have direct checkout data");
    assert_eq!(direct_checkouts, "[]");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM runs WHERE id = 1", [], |row| row
                .get::<_, i64>(0))
            .expect("deleted run count should be readable"),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("migration marker should be written"),
        1
    );
    assert!(legacy_workset_directory.join("keep.txt").exists());
}

#[test]
fn rolls_back_legacy_cleanup_when_a_later_step_fails() {
    let mut connection = Connection::open_in_memory().expect("database should open");
    create_legacy_schema(&connection);
    connection
        .execute(
            "INSERT INTO worksets (id, item_id, root_directory, branch)
                 VALUES (1, 1, '/tmp/legacy-workset', 'legacy-branch')",
            [],
        )
        .expect("legacy workset should be inserted");
    connection
        .execute(
            "INSERT INTO runs
                    (id, item_id, workset_id, workspace_id, machine_id, agent,
                     execution_profile, prompt, working_directory, session_name, pane_id,
                     started_at)
                 VALUES (1, 1, 1, NULL, 1, 'codex', 'implement', 'keep me', '/tmp',
                         'session-1', '%1', 123)",
            [],
        )
        .expect("legacy run should be inserted");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_workset_cleanup
                 BEFORE DELETE ON worksets
                 BEGIN
                     SELECT RAISE(ABORT, 'forced migration failure');
                 END;",
        )
        .expect("failure trigger should be created");

    let result = migrate_legacy_workset_data(&mut connection);
    assert!(result.is_err());

    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM worksets", [], |row| row
                .get::<_, i64>(0))
            .expect("workset count should be readable"),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get::<_, i64>(0))
            .expect("run count should be readable"),
        1
    );
    assert!(table_columns(&connection, "runs")
        .expect("runs table lookup should succeed")
        .contains(&"workset_id".to_owned()));
    assert!(!table_columns(&connection, "runs")
        .expect("runs table lookup should succeed")
        .contains(&"direct_checkouts_json".to_owned()));
    assert_eq!(
        connection
            .query_row(
                "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("migration marker should be readable"),
        0
    );
}

#[test]
fn round_trips_context_grill_defaults_and_run_snapshot() {
    let directory = tempdir().expect("temporary directory should exist");
    let database_path = directory.path().join("mission-manager.sqlite");
    let mut store = SqliteStore::open(&database_path).expect("database should open");
    let mut state = store.load_state().expect("initial state should load");

    state = apply_event(
        &mut store,
        state,
        Event::CreateContext {
            name: "Architecture".into(),
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::RegisterMachine {
            context_id: 1,
            name: "Local".into(),
            socket_name: "mission".into(),
            transport: MachineTransport::Local,
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::SetContextExecutionMachine {
            context_id: 1,
            machine_id: Some(1),
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::RegisterRepositoryAtLocation {
            project_id: 1,
            name: "mission-manager".into(),
            remote_url: "https://example.test/mission-manager".into(),
            base_branch: "main".into(),
            machine_id: 1,
            checkout_path: "/tmp/mission-manager".into(),
            worktree_root: "/tmp/worktrees".into(),
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::CreateItem {
            title: "Choose an architecture".into(),
            context_id: 1,
            project_id: 1,
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::CreateWorkspace {
            item_id: 1,
            repositories: vec![WorkspaceRepositoryInput {
                repository_id: 1,
                branch: "main".into(),
                base_branch: "main".into(),
            }],
        },
    );

    let configuration = GrillConfiguration {
        agent: AgentKind::Codex,
        model: "gpt-6-luna".into(),
        effort: "xhigh".into(),
    };
    state = apply_event(
        &mut store,
        state,
        Event::SetContextGrillDefaults {
            context_id: 1,
            defaults: configuration.clone(),
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::SetContextImplementDefaults {
            context_id: 1,
            defaults: GrillConfiguration {
                agent: AgentKind::Codex,
                model: "gpt-6-sol".into(),
                effort: "medium".into(),
            },
        },
    );
    let prompt = compose_grill_prompt(
        &state,
        1,
        &configuration,
        "Stress-test the proposed architecture.",
    )
    .expect("Grill prompt should compose");
    let _ = apply_event(
        &mut store,
        state,
        Event::StartGrillRun {
            item_id: 1,
            workspace_id: 1,
            repository_id: 1,
            machine_id: 1,
            configuration,
            prompt,
            skill_snapshot: grill_skill_snapshot().into(),
            working_directory: "/tmp/mission-manager".into(),
            session_name: "mission-item-1-grill-1".into(),
            pane_id: "%1".into(),
            started_at: 123,
            checkouts: vec![RunCheckout {
                repository_id: 1,
                path: "/tmp/mission-manager".into(),
                branch: "main".into(),
                is_dirty: false,
            }],
        },
    );
    store
        .connection
        .execute(
            "ALTER TABLE runs DROP COLUMN last_applied_agent_state_sequence",
            [],
        )
        .expect("the old schema should lack the sequence column");
    drop(store);
    let mut store = SqliteStore::open(&database_path)
        .expect("the database should add the nullable sequence column");
    state = store.load_state().expect("legacy Run should load");
    assert_eq!(state.runs[0].last_applied_agent_state_sequence, None);
    let question_group = parse_grill_question_group(
            "❓ Q1: Which direction?\n➡️ Keep the current design\nA) Keep it\nB) Replace it\n❓ Q2: What should we document?",
        )
        .expect("the Grill question group should parse");
    state = apply_event(
        &mut store,
        state,
        Event::RecordRunTranscript {
            run_id: 1,
            transcript: "raw Grill transcript".into(),
            question_group: Some(question_group),
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::RecordGrillAnswers {
            run_id: 1,
            answers: vec![
                GrillAnswer {
                    question_number: 1,
                    answer: "A. Keep it".into(),
                },
                GrillAnswer {
                    question_number: 2,
                    answer: "Document the migration path".into(),
                },
            ],
        },
    );
    let response =
        format_grill_response(&state.runs[0].grill_answers).expect("the response should format");
    state = apply_event(
        &mut store,
        state,
        Event::RecordGrillResponse {
            run_id: 1,
            response,
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::ApplyAgentStateReport {
            run_id: 1,
            state: crate::domain::RunState::Blocked,
            sequence: Some(41),
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::SetRunPaneStatus {
            run_id: 1,
            status: crate::domain::RunPaneStatus::Missing,
        },
    );
    let reloaded = store.load_state().expect("persisted state should load");
    assert_eq!(reloaded, state);
    assert_eq!(reloaded.contexts[0].grill_defaults.model, "gpt-6-luna");
    assert_eq!(reloaded.contexts[0].grill_defaults.effort, "xhigh");
    assert_eq!(reloaded.contexts[0].implement_defaults.model, "gpt-6-sol");
    assert_eq!(reloaded.contexts[0].implement_defaults.effort, "medium");
    assert_eq!(
        reloaded.runs[0].execution_profile,
        crate::domain::ExecutionProfile::Grill
    );
    assert_eq!(reloaded.runs[0].model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(reloaded.runs[0].effort.as_deref(), Some("xhigh"));
    assert_eq!(
        reloaded.runs[0].skill_snapshot.as_deref(),
        Some(grill_skill_snapshot())
    );
    assert_eq!(reloaded.runs[0].transcript, "raw Grill transcript");
    assert_eq!(reloaded.runs[0].grill_answers.len(), 2);
    assert_eq!(reloaded.runs[0].grill_decisions.len(), 2);
    assert_eq!(reloaded.runs[0].state, crate::domain::RunState::Blocked);
    assert_eq!(reloaded.runs[0].last_applied_agent_state_sequence, Some(41));
    assert_eq!(
        reloaded.runs[0].pane_status,
        crate::domain::RunPaneStatus::Missing
    );
    assert_eq!(
        reloaded.runs[0].grill_phase,
        Some(GrillPhase::RecoverablePaneLoss)
    );
    assert_eq!(
        reloaded.runs[0].grill_response.as_deref(),
        Some("1. A. Keep it\n2. Document the migration path")
    );
    state = apply_event(
        &mut store,
        state,
        Event::SetRunPaneStatus {
            run_id: 1,
            status: crate::domain::RunPaneStatus::Available,
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::UpdateRunState {
            run_id: 1,
            state: crate::domain::RunState::Finished,
        },
    );
    state = apply_event(
        &mut store,
        state,
        Event::ContinueGrill {
            run_id: 1,
            action: GrillContinuationAction::ToTickets,
            started_at: 100,
        },
    );
    let object = crate::provider::classify_url("https://github.com/acme/app/issues/7")
        .expect("the Issue URL should classify");
    let _state = apply_event(
        &mut store,
        state,
        Event::CaptureDownstreamIssues {
            run_id: 1,
            action: GrillContinuationAction::ToTickets,
            issues: vec![ConfirmedDownstreamIssue {
                object,
                snapshot: crate::domain::ExternalSnapshotData {
                    title: "Captured Issue".into(),
                    state: "OPEN".into(),
                    metadata: Vec::new(),
                    fetched_at: 456,
                },
                discovery: DownstreamIssueDiscovery::StructuredEvent,
            }],
        },
    );
    let reloaded_with_capture = store.load_state().expect("captured state should load");
    assert_eq!(
        reloaded_with_capture.runs[0].grill_action,
        Some(GrillContinuationAction::ToTickets)
    );
    assert_eq!(
        reloaded_with_capture.runs[0].last_applied_agent_state_sequence,
        Some(41)
    );
    assert_eq!(reloaded_with_capture.external_objects.len(), 1);
    assert_eq!(
        reloaded_with_capture.links[0].provenance,
        Some(LinkProvenance {
            run_id: 1,
            action: GrillContinuationAction::ToTickets,
            discovery: DownstreamIssueDiscovery::StructuredEvent,
        })
    );
}

#[test]
fn preserves_settings_executable_paths_audit_history_and_sequences() {
    let directory = tempdir().expect("temporary directory should exist");
    let database_path = directory.path().join("mission-manager.sqlite");
    let mut store = SqliteStore::open(&database_path).expect("database should open");

    assert_eq!(store.setting("theme").expect("setting should load"), None);
    store
        .set_setting("theme", "dark")
        .expect("setting should be written");
    store
        .set_setting("theme", "light")
        .expect("setting should update");
    store
        .set_executable_path("tmux_executable_path", Path::new("/custom/tmux"))
        .expect("executable path should be written");

    assert_eq!(
        store.setting("theme").expect("setting should load"),
        Some("light".into())
    );
    assert_eq!(
        store
            .executable_path("tmux_executable_path")
            .expect("executable path should load"),
        Some(Path::new("/custom/tmux").to_path_buf())
    );

    store
        .apply_with_audit(
            &[],
            &[
                AuditAction::ContextCreated { context_id: 1 },
                AuditAction::ProjectCreated { project_id: 1 },
            ],
        )
        .expect("audit actions should be written");

    assert_eq!(
        store.audit_entry_count().expect("audit count should load"),
        2
    );
    let history = store
        .list_audit_history()
        .expect("audit history should load");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, 2);
    assert_eq!(history[1].id, 1);
    assert!(matches!(
        history[0].action,
        AuditAction::ProjectCreated { project_id: 1 }
    ));

    let state = store.load_state().expect("state should load");
    assert_eq!(state.next_context_id, 2);
    let next_audit_id: i64 = store
        .connection
        .query_row(
            "SELECT value FROM metadata WHERE key = 'next_audit_id'",
            [],
            |row| row.get(0),
        )
        .expect("audit sequence should load");
    assert_eq!(next_audit_id, 3);
}
