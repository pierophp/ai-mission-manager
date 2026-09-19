pub mod domain;
pub mod persistence;
mod provider;

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
            app::list_context_attention_defaults,
            app::get_home,
            app::search_items_command,
            app::create_context,
            app::create_project,
            app::list_inbox_items,
            app::create_item,
            app::set_item_status,
            app::set_item_notes,
            app::add_item_reminder,
            app::remove_item_reminder,
            app::set_item_relation,
            app::link_external_object,
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
