use super::*;

pub(super) fn initialize_schema(connection: &mut Connection) -> Result<(), StoreError> {
    let contexts_table_existed = !table_columns(connection, "contexts")?.is_empty();
    let projects_table_existed = !table_columns(connection, "projects")?.is_empty();
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS metadata (
             key TEXT PRIMARY KEY NOT NULL,
             value INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS settings (
             key TEXT PRIMARY KEY NOT NULL,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS contexts (
             id INTEGER PRIMARY KEY NOT NULL,
             name TEXT NOT NULL UNIQUE,
             execution_machine_id INTEGER,
             grill_agent TEXT NOT NULL DEFAULT 'claude',
             grill_model TEXT NOT NULL DEFAULT 'claude-sonnet-4-5',
             grill_effort TEXT NOT NULL DEFAULT 'high',
             implement_agent TEXT NOT NULL DEFAULT 'claude',
             implement_model TEXT NOT NULL DEFAULT 'claude-sonnet-4-5',
             implement_effort TEXT NOT NULL DEFAULT 'high'
         );
         CREATE TABLE IF NOT EXISTS projects (
             id INTEGER PRIMARY KEY NOT NULL,
             context_id INTEGER NOT NULL REFERENCES contexts(id),
             name TEXT NOT NULL,
             default_item_status TEXT NOT NULL
                 CHECK (default_item_status IN ('Inbox', 'Active', 'Waiting', 'Done')),
             default_execution_mode TEXT NOT NULL DEFAULT 'worktree'
                 CHECK (default_execution_mode IN ('direct', 'worktree')),
             UNIQUE (context_id, name)
         );
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_context_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_project_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_item_number', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_repository_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_workspace_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_worktree_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_machine_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_run_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_external_object_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_link_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_activity_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_reminder_id', 1);
         INSERT OR IGNORE INTO metadata (key, value) VALUES ('next_audit_id', 1);",
    )?;

    if !contexts_table_existed {
        connection.execute("INSERT INTO contexts (id, name) VALUES (1, 'Personal')", [])?;
    }

    if !projects_table_existed {
        ensure_default_projects(connection)?;
    }

    let context_columns = table_columns(connection, "contexts")?;
    if !context_columns.is_empty() && !context_columns.iter().any(|column| column == "grill_agent")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN grill_agent TEXT NOT NULL DEFAULT 'claude'",
            [],
        )?;
    }
    if !context_columns.is_empty() && !context_columns.iter().any(|column| column == "grill_model")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN grill_model TEXT NOT NULL DEFAULT 'claude-sonnet-4-5'",
            [],
        )?;
    }
    if !context_columns.is_empty()
        && !context_columns
            .iter()
            .any(|column| column == "grill_effort")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN grill_effort TEXT NOT NULL DEFAULT 'high'",
            [],
        )?;
    }
    for (column, definition) in [
        ("implement_agent", "TEXT NOT NULL DEFAULT 'claude'"),
        (
            "implement_model",
            "TEXT NOT NULL DEFAULT 'claude-sonnet-4-5'",
        ),
        ("implement_effort", "TEXT NOT NULL DEFAULT 'high'"),
    ] {
        if !context_columns.is_empty() && !context_columns.iter().any(|existing| existing == column)
        {
            connection.execute(
                &format!("ALTER TABLE contexts ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }

    connection.execute(
        "UPDATE contexts
         SET grill_model = CASE grill_model
             WHEN 'claude-opus-4-1' THEN 'claude-opus-5'
             WHEN 'codex-sol' THEN 'gpt-6-sol'
             WHEN 'codex-terra' THEN 'gpt-6-sol'
             WHEN 'codex-luna' THEN 'gpt-6-luna'
             ELSE grill_model
         END
         WHERE grill_model IN (
             'claude-opus-4-1', 'codex-sol', 'codex-terra', 'codex-luna'
         )",
        [],
    )?;

    if table_columns(connection, "items")?.is_empty() {
        create_items_table(connection)?;
    }

    let project_columns = table_columns(connection, "projects")?;
    if !project_columns.is_empty()
        && !project_columns
            .iter()
            .any(|column| column == "default_execution_mode")
    {
        connection.execute(
            "ALTER TABLE projects ADD COLUMN default_execution_mode TEXT NOT NULL DEFAULT 'worktree'",
            [],
        )?;
    }

    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS projects_by_context
             ON projects (context_id);
         CREATE INDEX IF NOT EXISTS items_by_project_and_status
             ON items (project_id, status);
         CREATE TABLE IF NOT EXISTS repositories (
             id INTEGER PRIMARY KEY NOT NULL,
             project_id INTEGER NOT NULL REFERENCES projects(id),
             name TEXT NOT NULL,
             remote_url TEXT NOT NULL,
             base_branch TEXT NOT NULL DEFAULT 'main',
             UNIQUE (project_id, name)
         );
         CREATE INDEX IF NOT EXISTS repositories_by_project
             ON repositories (project_id, id);
         CREATE TABLE IF NOT EXISTS machines (
             id INTEGER PRIMARY KEY NOT NULL,
             context_id INTEGER NOT NULL REFERENCES contexts(id),
             name TEXT NOT NULL,
             socket_name TEXT NOT NULL,
             transport_json TEXT NOT NULL DEFAULT '{\"kind\":\"local\"}',
             last_observed TEXT NOT NULL DEFAULT 'unknown',
             last_observed_at INTEGER,
             UNIQUE (context_id, name)
         );
         CREATE INDEX IF NOT EXISTS machines_by_context
             ON machines (context_id, id);
         CREATE TABLE IF NOT EXISTS repository_locations (
             repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
             machine_id INTEGER NOT NULL REFERENCES machines(id) ON DELETE CASCADE,
             checkout_path TEXT NOT NULL,
             worktree_root TEXT NOT NULL,
             PRIMARY KEY (repository_id, machine_id)
         );
         CREATE INDEX IF NOT EXISTS repository_locations_by_machine
             ON repository_locations (machine_id, repository_id);
         CREATE TABLE IF NOT EXISTS workspaces (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             preparation_state TEXT NOT NULL DEFAULT 'pending'
         );
         CREATE INDEX IF NOT EXISTS workspaces_by_item
             ON workspaces (item_id, id);
         CREATE TABLE IF NOT EXISTS workspace_repositories (
             workspace_id INTEGER NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
             repository_id INTEGER NOT NULL REFERENCES repositories(id),
             branch TEXT NOT NULL,
             base_branch TEXT NOT NULL,
             PRIMARY KEY (workspace_id, repository_id)
         );
         CREATE INDEX IF NOT EXISTS workspace_repositories_by_repository
             ON workspace_repositories (repository_id);
         CREATE TABLE IF NOT EXISTS worktrees (
             id INTEGER PRIMARY KEY NOT NULL,
             workspace_id INTEGER NOT NULL REFERENCES workspaces(id),
             repository_id INTEGER NOT NULL REFERENCES repositories(id),
             machine_id INTEGER NOT NULL REFERENCES machines(id),
             path TEXT NOT NULL,
             branch TEXT NOT NULL,
             base_branch TEXT NOT NULL,
             is_dirty INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS worktrees_by_workspace
             ON worktrees (workspace_id, id);
         CREATE INDEX IF NOT EXISTS worktrees_by_repository
             ON worktrees (repository_id, id);
         CREATE TABLE IF NOT EXISTS runs (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id),
             workspace_id INTEGER REFERENCES workspaces(id),
             repository_id INTEGER REFERENCES repositories(id),
             worktree_id INTEGER REFERENCES worktrees(id),
             machine_id INTEGER NOT NULL REFERENCES machines(id),
             agent TEXT NOT NULL CHECK (agent IN ('claude', 'codex')),
             execution_profile TEXT NOT NULL
                 CHECK (execution_profile IN ('investigate', 'implement', 'review', 'custom', 'grill')),
             model TEXT,
             effort TEXT,
             skill_snapshot TEXT,
             prompt TEXT NOT NULL,
             working_directory TEXT NOT NULL,
             session_name TEXT NOT NULL,
             pane_id TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             state TEXT NOT NULL DEFAULT 'unknown',
             last_applied_agent_state_sequence INTEGER,
             pane_status TEXT NOT NULL DEFAULT 'unknown',
             direct_checkouts_json TEXT NOT NULL DEFAULT '[]',
             transcript TEXT NOT NULL DEFAULT '',
             grill_question_group_json TEXT,
             grill_answers_json TEXT NOT NULL DEFAULT '[]',
             grill_decisions_json TEXT NOT NULL DEFAULT '[]',
             grill_response TEXT,
             grill_phase TEXT,
             grill_action TEXT,
             grill_action_started_at INTEGER
             ,implementation_queue_id INTEGER
             ,implementation_queue_position INTEGER
         );
         CREATE TABLE IF NOT EXISTS implementation_queues (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             queue_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS runs_by_item
             ON runs (item_id, id);
         CREATE INDEX IF NOT EXISTS runs_by_workspace
             ON runs (workspace_id, id);
         CREATE TABLE IF NOT EXISTS reminders (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             remind_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS reminders_by_item
             ON reminders (item_id, id);
         CREATE TABLE IF NOT EXISTS item_relationships (
             from_item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             to_item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             kind TEXT NOT NULL CHECK (kind IN ('blocks', 'blocked_by', 'related_to')),
             PRIMARY KEY (from_item_id, to_item_id, kind)
         );
         CREATE INDEX IF NOT EXISTS relationships_by_target
             ON item_relationships (to_item_id);
         CREATE TABLE IF NOT EXISTS external_objects (
             id INTEGER PRIMARY KEY NOT NULL,
             provider TEXT NOT NULL CHECK (provider IN ('github', 'generic')),
             kind TEXT NOT NULL CHECK (kind IN ('issue', 'pull_request', 'generic')),
             external_key TEXT NOT NULL,
             canonical_url TEXT NOT NULL,
             UNIQUE (provider, external_key)
         );
         CREATE TABLE IF NOT EXISTS external_links (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             external_object_id INTEGER NOT NULL REFERENCES external_objects(id) ON DELETE CASCADE,
             purpose TEXT NOT NULL DEFAULT 'others',
             UNIQUE (item_id, external_object_id)
         );
         CREATE TABLE IF NOT EXISTS external_snapshots (
             external_object_id INTEGER PRIMARY KEY NOT NULL REFERENCES external_objects(id) ON DELETE CASCADE,
             title TEXT NOT NULL,
             state TEXT NOT NULL,
             metadata_json TEXT NOT NULL,
             fetched_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS link_attention_state (
             link_id INTEGER PRIMARY KEY NOT NULL REFERENCES external_links(id) ON DELETE CASCADE,
             reviewed_activity_id INTEGER NOT NULL DEFAULT 0,
             title_attention INTEGER,
             state_attention INTEGER,
             metadata_attention INTEGER,
             watch_until TEXT,
             review_at TEXT,
             provenance_json TEXT
         );
         CREATE TABLE IF NOT EXISTS activities (
             id INTEGER PRIMARY KEY NOT NULL,
             external_object_id INTEGER NOT NULL REFERENCES external_objects(id) ON DELETE CASCADE,
             observed_at INTEGER NOT NULL,
             changes_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS context_attention_defaults (
             context_id INTEGER NOT NULL REFERENCES contexts(id) ON DELETE CASCADE,
             object_kind TEXT NOT NULL CHECK (object_kind IN ('issue', 'pull_request', 'generic')),
             title_attention INTEGER NOT NULL,
             state_attention INTEGER NOT NULL,
             metadata_attention INTEGER NOT NULL,
             PRIMARY KEY (context_id, object_kind)
         );
         CREATE INDEX IF NOT EXISTS external_links_by_item
             ON external_links (item_id);
         CREATE INDEX IF NOT EXISTS external_links_by_object
             ON external_links (external_object_id);
         CREATE INDEX IF NOT EXISTS activities_by_object
             ON activities (external_object_id, id);
         CREATE TABLE IF NOT EXISTS audit_entries (
             id INTEGER PRIMARY KEY NOT NULL,
             recorded_at INTEGER NOT NULL,
             action_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS audit_entries_by_recorded_at
             ON audit_entries (recorded_at, id);",
    )?;

    let external_link_columns = table_columns(connection, "external_links")?;
    if !external_link_columns
        .iter()
        .any(|column| column == "purpose")
    {
        connection.execute(
            "ALTER TABLE external_links ADD COLUMN purpose TEXT NOT NULL DEFAULT 'others'",
            [],
        )?;
        let legacy_action = "json_extract(
            (SELECT provenance_json FROM link_attention_state
             WHERE link_attention_state.link_id = external_links.id), '$.action')";
        let purpose_backfill = if external_link_columns
            .iter()
            .any(|column| column == "is_spec")
        {
            format!(
                "CASE
                     WHEN external_links.is_spec != 0 THEN 'to-spec'
                     WHEN {legacy_action} = 'to-tickets' THEN 'to-tickets'
                     ELSE 'others'
                 END"
            )
        } else {
            format!(
                "CASE {legacy_action}
                     WHEN 'to-spec' THEN 'to-spec'
                     WHEN 'to-tickets' THEN 'to-tickets'
                     ELSE 'others'
                 END"
            )
        };
        connection.execute(
            &format!(
                "UPDATE external_links
                 SET purpose = ({purpose_backfill})"
            ),
            [],
        )?;
    }

    let context_columns = table_columns(connection, "contexts")?;
    if !context_columns
        .iter()
        .any(|column| column == "execution_machine_id")
    {
        connection.execute(
            "ALTER TABLE contexts ADD COLUMN execution_machine_id INTEGER",
            [],
        )?;
        connection.execute(
            "UPDATE contexts
             SET execution_machine_id = (
                 SELECT MIN(id) FROM machines WHERE machines.context_id = contexts.id
             )
             WHERE execution_machine_id IS NULL
               AND (SELECT COUNT(*) FROM machines WHERE machines.context_id = contexts.id) = 1",
            [],
        )?;
    }

    let repository_columns = table_columns(connection, "repositories")?;
    if !repository_columns.is_empty()
        && !repository_columns
            .iter()
            .any(|column| column == "base_branch")
    {
        connection.execute(
            "ALTER TABLE repositories ADD COLUMN base_branch TEXT NOT NULL DEFAULT 'main'",
            [],
        )?;
    }
    let workspace_columns = table_columns(connection, "workspaces")?;
    if !workspace_columns.is_empty()
        && !workspace_columns
            .iter()
            .any(|column| column == "preparation_state")
    {
        connection.execute(
            "ALTER TABLE workspaces ADD COLUMN preparation_state TEXT NOT NULL DEFAULT 'pending'",
            [],
        )?;
    }
    let run_columns = table_columns(connection, "runs")?;
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "state") {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN state TEXT NOT NULL DEFAULT 'unknown'",
            [],
        )?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "last_applied_agent_state_sequence")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN last_applied_agent_state_sequence INTEGER",
            [],
        )?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "pane_status") {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN pane_status TEXT NOT NULL DEFAULT 'unknown'",
            [],
        )?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "direct_checkouts_json")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN direct_checkouts_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "transcript") {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN transcript TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "grill_question_group_json")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN grill_question_group_json TEXT",
            [],
        )?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "grill_answers_json")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN grill_answers_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "grill_decisions_json")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN grill_decisions_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "grill_response") {
        connection.execute("ALTER TABLE runs ADD COLUMN grill_response TEXT", [])?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "grill_phase") {
        connection.execute("ALTER TABLE runs ADD COLUMN grill_phase TEXT", [])?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "grill_action") {
        connection.execute("ALTER TABLE runs ADD COLUMN grill_action TEXT", [])?;
    }
    if !run_columns.is_empty()
        && !run_columns
            .iter()
            .any(|column| column == "grill_action_started_at")
    {
        connection.execute(
            "ALTER TABLE runs ADD COLUMN grill_action_started_at INTEGER",
            [],
        )?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "repository_id") {
        connection.execute("ALTER TABLE runs ADD COLUMN repository_id INTEGER", [])?;
    }
    if !run_columns.is_empty() && !run_columns.iter().any(|column| column == "worktree_id") {
        connection.execute("ALTER TABLE runs ADD COLUMN worktree_id INTEGER", [])?;
    }
    ensure_sequence_at_least(connection, "next_context_id", "contexts", "id")?;
    ensure_sequence_at_least(connection, "next_project_id", "projects", "id")?;
    ensure_sequence_at_least(connection, "next_item_id", "items", "id")?;
    ensure_sequence_at_least(connection, "next_repository_id", "repositories", "id")?;
    ensure_sequence_at_least(connection, "next_workspace_id", "workspaces", "id")?;
    ensure_sequence_at_least(connection, "next_worktree_id", "worktrees", "id")?;
    ensure_sequence_at_least(connection, "next_machine_id", "machines", "id")?;
    ensure_sequence_at_least(connection, "next_run_id", "runs", "id")?;
    ensure_sequence_at_least(
        connection,
        "next_external_object_id",
        "external_objects",
        "id",
    )?;
    ensure_sequence_at_least(connection, "next_link_id", "external_links", "id")?;
    ensure_sequence_at_least(connection, "next_activity_id", "activities", "id")?;
    ensure_sequence_at_least(connection, "next_reminder_id", "reminders", "id")?;
    ensure_sequence_at_least(connection, "next_audit_id", "audit_entries", "id")?;

    Ok(())
}

/// Remove data owned by the legacy Workset model exactly once.
///
/// This deliberately only changes SQLite state. Any old Workset directories are
/// left for explicit human cleanup, and the transaction keeps all database
/// records intact if one cleanup step fails.
pub(super) fn migrate_legacy_workset_data(connection: &mut Connection) -> Result<(), StoreError> {
    if table_columns(connection, "metadata")?.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    let worksets_exist = !table_columns(&transaction, "worksets")?.is_empty();
    let workset_repositories_exist =
        !table_columns(&transaction, "workset_repositories")?.is_empty();
    let run_columns = table_columns(&transaction, "runs")?;
    let runs_have_legacy_link = run_columns.iter().any(|column| column == "workset_id");
    let completed: Option<i64> = transaction
        .query_row(
            "SELECT value FROM metadata WHERE key = 'workset_migration_completed'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if completed == Some(1)
        && !worksets_exist
        && !workset_repositories_exist
        && !runs_have_legacy_link
    {
        return Ok(());
    }

    if runs_have_legacy_link {
        transaction.execute("DELETE FROM runs WHERE workset_id IS NOT NULL", [])?;
    }
    if worksets_exist {
        transaction.execute(
            "DELETE FROM activities
             WHERE external_object_id IN (
                 SELECT DISTINCT external_links.external_object_id
                 FROM external_links
                 JOIN worksets ON worksets.item_id = external_links.item_id
             )",
            [],
        )?;
    }
    transaction.execute(
        "DELETE FROM audit_entries WHERE LOWER(action_json) LIKE '%workset%'",
        [],
    )?;
    if workset_repositories_exist {
        transaction.execute("DELETE FROM workset_repositories", [])?;
    }
    if worksets_exist {
        transaction.execute("DELETE FROM worksets", [])?;
    }
    if runs_have_legacy_link {
        let direct_checkouts_select = if run_columns
            .iter()
            .any(|column| column == "direct_checkouts_json")
        {
            "direct_checkouts_json"
        } else {
            "'[]'"
        };
        transaction.execute_batch(&format!(
            "ALTER TABLE runs RENAME TO runs_legacy;
             CREATE TABLE runs (
                 id INTEGER PRIMARY KEY NOT NULL,
                 item_id INTEGER NOT NULL REFERENCES items(id),
                 workspace_id INTEGER REFERENCES workspaces(id),
                 repository_id INTEGER REFERENCES repositories(id),
                 worktree_id INTEGER REFERENCES worktrees(id),
                 machine_id INTEGER NOT NULL REFERENCES machines(id),
                 agent TEXT NOT NULL CHECK (agent IN ('claude', 'codex')),
                 execution_profile TEXT NOT NULL
                     CHECK (execution_profile IN ('investigate', 'implement', 'review', 'custom')),
                 prompt TEXT NOT NULL,
                 working_directory TEXT NOT NULL,
                 session_name TEXT NOT NULL,
                 pane_id TEXT NOT NULL,
                 started_at INTEGER NOT NULL,
                 state TEXT NOT NULL DEFAULT 'unknown',
                 pane_status TEXT NOT NULL DEFAULT 'unknown',
                 direct_checkouts_json TEXT NOT NULL DEFAULT '[]'
             );
             INSERT INTO runs (
                 id, item_id, workspace_id, repository_id, worktree_id, machine_id, agent,
                 execution_profile, prompt, working_directory, session_name, pane_id, started_at,
                 state, pane_status, direct_checkouts_json
             )
             SELECT id, item_id, NULL, NULL, NULL, machine_id, agent, execution_profile,
                    prompt, working_directory, session_name, pane_id, started_at,
                    state, pane_status, {direct_checkouts_select}
             FROM runs_legacy;
             DROP TABLE runs_legacy;"
        ))?;
    }
    if workset_repositories_exist {
        transaction.execute_batch("DROP INDEX IF EXISTS workset_repositories_by_repository; DROP TABLE workset_repositories;")?;
    }
    if worksets_exist {
        transaction.execute_batch("DROP INDEX IF EXISTS worksets_by_item; DROP TABLE worksets;")?;
    }
    transaction.execute(
        "INSERT INTO metadata (key, value) VALUES ('workset_migration_completed', 1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn migrate_runs_for_grill(connection: &mut Connection) -> Result<(), StoreError> {
    let columns = table_columns(connection, "runs")?;
    if columns.is_empty() || columns.iter().any(|column| column == "model") {
        return Ok(());
    }

    connection.execute_batch(
        "DROP INDEX IF EXISTS runs_by_item;
         DROP INDEX IF EXISTS runs_by_workspace;
         ALTER TABLE runs RENAME TO runs_legacy;
         CREATE TABLE runs (
             id INTEGER PRIMARY KEY NOT NULL,
             item_id INTEGER NOT NULL REFERENCES items(id),
             workspace_id INTEGER REFERENCES workspaces(id),
             repository_id INTEGER REFERENCES repositories(id),
             worktree_id INTEGER REFERENCES worktrees(id),
             machine_id INTEGER NOT NULL REFERENCES machines(id),
             agent TEXT NOT NULL CHECK (agent IN ('claude', 'codex')),
             execution_profile TEXT NOT NULL
                 CHECK (execution_profile IN ('investigate', 'implement', 'review', 'custom', 'grill')),
             model TEXT,
             effort TEXT,
             skill_snapshot TEXT,
             prompt TEXT NOT NULL,
             working_directory TEXT NOT NULL,
             session_name TEXT NOT NULL,
             pane_id TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             state TEXT NOT NULL DEFAULT 'unknown',
             pane_status TEXT NOT NULL DEFAULT 'unknown',
             direct_checkouts_json TEXT NOT NULL DEFAULT '[]'
         );
         INSERT INTO runs (
             id, item_id, workspace_id, repository_id, worktree_id, machine_id, agent,
             execution_profile, prompt, working_directory, session_name, pane_id, started_at,
             state, pane_status, direct_checkouts_json
         )
         SELECT id, item_id, workspace_id, repository_id, worktree_id, machine_id, agent,
                execution_profile, prompt, working_directory, session_name, pane_id, started_at,
                state, pane_status, direct_checkouts_json
         FROM runs_legacy;
         DROP TABLE runs_legacy;
         CREATE INDEX runs_by_item ON runs (item_id, id);
         CREATE INDEX runs_by_workspace ON runs (workspace_id, id);",
    )?;
    Ok(())
}

fn ensure_default_projects(connection: &mut Connection) -> Result<(), StoreError> {
    let context_ids = {
        let mut statement = connection.prepare("SELECT id FROM contexts ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for context_id in context_ids {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM projects WHERE context_id = ?1 AND name = 'Default'
             )",
            params![context_id],
            |row| row.get(0),
        )?;
        if exists {
            continue;
        }

        let project_id: i64 = connection.query_row(
            "SELECT value FROM metadata WHERE key = 'next_project_id'",
            [],
            |row| row.get(0),
        )?;
        let next_project_id = project_id
            .checked_add(1)
            .ok_or(StoreError::SequenceExhausted)?;
        connection.execute(
            "INSERT INTO projects (id, context_id, name, default_item_status)
             VALUES (?1, ?2, 'Default', 'Inbox')",
            params![project_id, context_id],
        )?;
        connection.execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'next_project_id'",
            params![next_project_id],
        )?;
    }

    Ok(())
}

fn create_items_table(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE items (
             id INTEGER PRIMARY KEY NOT NULL,
             human_identifier TEXT NOT NULL UNIQUE,
             title TEXT NOT NULL,
             project_id INTEGER NOT NULL REFERENCES projects(id),
             status TEXT NOT NULL CHECK (status IN ('Inbox', 'Active', 'Waiting', 'Done')),
             notes TEXT NOT NULL DEFAULT ''
         );",
    )?;
    Ok(())
}

pub(super) fn table_columns(
    connection: &Connection,
    table: &str,
) -> Result<Vec<String>, StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn ensure_sequence_at_least(
    connection: &Connection,
    key: &str,
    table: &str,
    column: &str,
) -> Result<(), StoreError> {
    let minimum: i64 = connection.query_row(
        &format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}"),
        [],
        |row| row.get(0),
    )?;
    connection.execute(
        "UPDATE metadata SET value = CASE WHEN value < ?1 THEN ?1 ELSE value END
         WHERE key = ?2",
        params![minimum, key],
    )?;
    Ok(())
}
