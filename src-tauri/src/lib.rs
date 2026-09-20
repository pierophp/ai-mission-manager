mod agent_state;
mod dependencies;
pub mod domain;
mod git;
pub mod persistence;
mod provider;
mod terminal;

mod app;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database_path = data_dir.join("mission-manager.sqlite");
            let runtime = app::Runtime::open(database_path)?;
            app.manage(std::sync::Mutex::new(runtime));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::list_contexts,
            app::list_projects,
            app::list_repositories,
            app::list_machines,
            app::get_setup_state,
            app::complete_setup,
            app::get_health_status,
            app::list_context_attention_defaults,
            app::get_home,
            app::reconcile_runs,
            app::search_items_command,
            app::create_context,
            app::create_project,
            app::register_repository,
            app::register_machine,
            app::check_machine,
            app::create_workset,
            app::attach_workset,
            app::add_repository_to_workset,
            app::set_workset_archived,
            app::prepare_workset_removal,
            app::remove_workset,
            app::prepare_item_deletion,
            app::delete_item,
            app::list_inbox_items,
            app::create_item,
            app::compose_run_prompt,
            app::start_run,
            app::list_run_suggestions,
            app::attach_run,
            app::list_workset_panes,
            app::open_terminal,
            app::terminal_input,
            app::terminal_resize,
            app::close_terminal,
            app::open_external_terminal,
            app::stop_run,
            app::list_audit_history,
            app::set_item_status,
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

pub fn run_agent_state_hook(args: &[String]) -> Result<(), String> {
    let provider = args
        .first()
        .ok_or_else(|| "usage: --agent-state-hook <claude|codex>".to_owned())?;
    agent_state::run_hook_from_stdin(provider)
}
