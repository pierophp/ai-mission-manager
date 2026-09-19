use std::{path::Path, sync::Mutex};

use tauri::State;

use crate::{
    domain::{
        decide, home_view, search_items, Context, DomainState, Event, HomeView, Item, ItemRelation,
        ItemRelationKind, ItemStatus, ItemView, Project, ProjectDefaults,
    },
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

    fn create_context(&mut self, name: String) -> Result<Context, String> {
        let decision = decide(self.state.clone(), Event::CreateContext { name })
            .map_err(|error| error.to_string())?;
        let context = decision
            .state
            .contexts
            .last()
            .cloned()
            .ok_or_else(|| "Context creation produced no Context".to_owned())?;
        self.commit(decision)?;
        Ok(context)
    }

    fn create_project(
        &mut self,
        name: String,
        context_id: i64,
        defaults: ProjectDefaults,
    ) -> Result<Project, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateProject {
                context_id,
                name,
                defaults,
            },
        )
        .map_err(|error| error.to_string())?;
        let project = decision
            .state
            .projects
            .last()
            .cloned()
            .ok_or_else(|| "Project creation produced no Project".to_owned())?;
        self.commit(decision)?;
        Ok(project)
    }

    fn create_item(
        &mut self,
        title: String,
        context_id: i64,
        project_id: i64,
    ) -> Result<Item, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateItem {
                title,
                context_id,
                project_id,
            },
        )
        .map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .last()
            .cloned()
            .ok_or_else(|| "Item creation produced no Item".to_owned())?;
        self.commit(decision)?;
        Ok(item)
    }

    fn update_item(&mut self, event: Event, item_id: i64) -> Result<Item, String> {
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| "Item update produced no Item".to_owned())?;
        self.commit(decision)?;
        Ok(item)
    }

    fn set_item_relation(
        &mut self,
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    ) -> Result<ItemRelation, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetItemRelation {
                from_item_id,
                to_item_id,
                kind,
            },
        )
        .map_err(|error| error.to_string())?;
        let relation = decision
            .state
            .relationships
            .last()
            .cloned()
            .ok_or_else(|| "Item relationship produced no relationship".to_owned())?;
        self.commit(decision)?;
        Ok(relation)
    }

    fn commit(&mut self, decision: crate::domain::Decision) -> Result<(), String> {
        self.store
            .apply(&decision.effects)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        Ok(())
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
pub fn list_projects(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Project>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.projects.clone())
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_home(
    context_id: Option<i64>,
    now: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HomeView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| home_view(&runtime.state, context_id, &now))
}

#[tauri::command(rename_all = "camelCase")]
pub fn search_items_command(
    query: String,
    context_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ItemView>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| search_items(&runtime.state, &query, context_id))
}

#[tauri::command]
pub fn create_context(name: String, state: State<'_, Mutex<Runtime>>) -> Result<Context, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_context(name)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_project(
    name: String,
    context_id: i64,
    default_item_status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_project(
            name,
            context_id,
            ProjectDefaults {
                item_status: default_item_status,
            },
        )
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
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_item(title, context_id, project_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_status(
    item_id: i64,
    status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemStatus { item_id, status }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_notes(
    item_id: i64,
    notes: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemNotes { item_id, notes }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_reminder(
    item_id: i64,
    reminder_at: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(
            Event::SetItemReminder {
                item_id,
                reminder_at,
            },
            item_id,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_relation(
    from_item_id: i64,
    to_item_id: i64,
    kind: ItemRelationKind,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemRelation, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_item_relation(from_item_id, to_item_id, kind)
}
