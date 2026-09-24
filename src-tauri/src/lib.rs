mod agent_state;
mod dependencies;
pub mod domain;
mod git;
pub mod persistence;
mod provider;
mod terminal;

mod app;
mod features;

use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::Manager;

fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

fn database_path(home_dir: &Path, legacy_data_dir: &Path) -> std::io::Result<PathBuf> {
    let data_dir = home_dir.join(".ai-mission-manager");
    fs::create_dir_all(&data_dir)?;

    let database_path = data_dir.join("mission-manager.sqlite");
    let legacy_database_path = legacy_data_dir.join("mission-manager.sqlite");
    if database_path.exists() || !legacy_database_path.exists() {
        return Ok(database_path);
    }

    let mut moved_sidecars = Vec::new();
    for suffix in ["-wal", "-shm"] {
        let source = sqlite_sidecar(&legacy_database_path, suffix);
        if !source.exists() {
            continue;
        }

        let destination = sqlite_sidecar(&database_path, suffix);
        if let Err(error) = fs::rename(&source, &destination) {
            for (moved_source, moved_destination) in moved_sidecars.into_iter().rev() {
                let _ = fs::rename(moved_destination, moved_source);
            }
            return Err(error);
        }
        moved_sidecars.push((source, destination));
    }

    if let Err(error) = fs::rename(&legacy_database_path, &database_path) {
        for (source, destination) in moved_sidecars.into_iter().rev() {
            let _ = fs::rename(destination, source);
        }
        return Err(error);
    }

    Ok(database_path)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            let database_path =
                database_path(&app.path().home_dir()?, &app.path().app_data_dir()?)?;
            let runtime = features::Runtime::open(database_path)?;
            app.manage(features::SharedRuntime::new(runtime));
            Ok(())
        })
        // Keep the stable `app::*` facade paths in the command registry while
        // feature modules own the implementations. Tauri's generated command
        // wrapper macros are path-sensitive, so this preserves registration
        // without exposing feature implementation paths as a contract.
        .invoke_handler(tauri::generate_handler![
            app::list_contexts,
            app::list_grill_model_catalog,
            app::set_context_grill_defaults,
            app::list_projects,
            app::list_repositories,
            app::list_repository_locations,
            app::list_machines,
            app::get_setup_state,
            app::complete_setup,
            app::get_health_status,
            app::list_context_attention_defaults,
            app::get_home,
            app::reconcile_runs,
            app::search_items_command,
            app::create_context,
            app::update_context,
            app::set_context_execution_machine,
            app::create_project,
            app::update_project,
            app::register_repository,
            app::update_repository,
            app::register_repository_at_location,
            app::update_repository_location,
            app::prepare_project_deletion,
            app::delete_project,
            app::prepare_context_deletion,
            app::delete_context,
            app::prepare_reset_local_data,
            app::reset_all_local_data,
            app::register_machine,
            app::update_machine,
            app::check_machine,
            app::create_worktree,
            app::prepare_worktree,
            app::prepare_worktree_removal,
            app::remove_worktree,
            app::attach_worktree,
            app::prepare_repository_deletion,
            app::delete_repository,
            app::prepare_machine_deletion,
            app::delete_machine,
            app::prepare_item_deletion,
            app::delete_item,
            app::prepare_external_object_deletion,
            app::delete_external_object,
            app::unlink_external_link,
            app::delete_run,
            app::list_inbox_items,
            app::create_item,
            app::compose_run_prompt,
            app::compose_grill_prompt,
            app::prepare_direct_run,
            app::prepare_grill_run,
            app::start_direct_run,
            app::start_grill_run,
            app::start_worktree_run,
            app::list_run_suggestions,
            app::attach_run,
            app::stop_untracked_agent,
            app::delete_untracked_agent,
            app::open_terminal,
            app::terminal_input,
            app::submit_grill_answers,
            app::continue_grill,
            app::terminal_resize,
            app::close_terminal,
            app::open_external_terminal,
            app::stop_run,
            app::finish_run,
            app::list_audit_history,
            app::get_activity_tab,
            app::set_item_status,
            app::set_item_title,
            app::set_item_notes,
            app::add_item_reminder,
            app::remove_item_reminder,
            app::set_item_relation,
            app::link_external_object,
            app::create_github_issue,
            app::add_external_comment,
            app::refresh_external_object,
            app::poll_external_objects,
            app::set_link_attention_policy,
            app::set_link_watch_until,
            app::set_link_review_at,
            app::clear_link_review_at,
            app::set_context_attention_default,
            app::mark_link_reviewed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI Mission Manager");
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{database_path, sqlite_sidecar};

    #[test]
    fn stores_the_database_under_the_home_directory_and_migrates_the_legacy_file() {
        let root = tempdir().expect("temporary root should be created");
        let legacy = root.path().join("legacy-app-data");
        fs::create_dir_all(&legacy).expect("legacy directory should be created");
        let legacy_database = legacy.join("mission-manager.sqlite");
        fs::write(&legacy_database, b"database").expect("legacy database should be created");
        fs::write(sqlite_sidecar(&legacy_database, "-wal"), b"wal")
            .expect("legacy WAL should be created");

        let database = database_path(root.path(), &legacy).expect("database path should resolve");
        let expected = root
            .path()
            .join(".ai-mission-manager/mission-manager.sqlite");
        assert_eq!(database, expected);
        assert_eq!(
            fs::read(&database).expect("migrated database should exist"),
            b"database"
        );
        assert_eq!(
            fs::read(sqlite_sidecar(&database, "-wal")).expect("migrated WAL should exist"),
            b"wal"
        );
        assert!(!legacy_database.exists());
    }

    #[test]
    fn keeps_the_new_database_when_both_locations_exist() {
        let root = tempdir().expect("temporary root should be created");
        let legacy = root.path().join("legacy-app-data");
        fs::create_dir_all(&legacy).expect("legacy directory should be created");
        let new_data_dir = root.path().join(".ai-mission-manager");
        fs::create_dir_all(&new_data_dir).expect("new data directory should be created");
        fs::write(new_data_dir.join("mission-manager.sqlite"), b"new")
            .expect("new database should be created");
        let legacy_database = legacy.join("mission-manager.sqlite");
        fs::write(&legacy_database, b"legacy").expect("legacy database should be created");

        let database = database_path(root.path(), &legacy).expect("database path should resolve");
        assert_eq!(
            fs::read(database).expect("new database should remain"),
            b"new"
        );
        assert_eq!(
            fs::read(legacy_database).expect("legacy database should remain"),
            b"legacy"
        );
    }
}
