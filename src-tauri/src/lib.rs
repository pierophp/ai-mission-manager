pub mod domain;
pub mod persistence;

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
            app::create_context,
            app::create_project,
            app::list_inbox_items,
            app::create_item,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI Mission Manager");
}
