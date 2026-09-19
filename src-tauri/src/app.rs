use std::{path::Path, sync::Mutex};

use tauri::State;

use crate::{
    domain::{decide, Context, DomainState, Event, Item, ItemStatus},
    persistence::SqliteStore,
};

pub struct Runtime {
    store: SqliteStore,
    state: DomainState,
}

impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let store = SqliteStore::open(path).map_err(|error| error.to_string())?;
        let state = store.load_state().map_err(|error| error.to_string())?;
        Ok(Self { store, state })
    }

    fn create_item(&mut self, title: String, context_id: i64) -> Result<Item, String> {
        let decision = decide(self.state.clone(), Event::CreateItem { title, context_id })
            .map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .last()
            .cloned()
            .ok_or_else(|| "Item creation produced no Item".to_owned())?;
        self.store
            .apply(&decision.effects)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        Ok(item)
    }
}

#[tauri::command]
pub fn list_contexts(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Context>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.contexts.clone())
}

#[tauri::command]
pub fn list_inbox_items(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Item>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| {
            runtime
                .state
                .items
                .iter()
                .filter(|item| item.status == ItemStatus::Inbox)
                .cloned()
                .collect()
        })
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_item(
    title: String,
    context_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_item(title, context_id)
}
