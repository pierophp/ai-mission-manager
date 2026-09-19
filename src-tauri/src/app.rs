use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::State;

use crate::{
    domain::{
        decide, external_link_view, home_view, search_items, Context, DomainState, Event,
        ExternalLinkView, ExternalObjectInput, ExternalProvider, ExternalSnapshot, HomeView, Item,
        ItemRelation, ItemRelationKind, ItemStatus, ItemView, Project, ProjectDefaults,
    },
    persistence::SqliteStore,
    provider::{classify_url, resolve_gh_executable, GithubCli},
};

pub struct Runtime {
    store: SqliteStore,
    state: DomainState,
    gh_executable_path: Option<PathBuf>,
}

impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let mut store = SqliteStore::open(path).map_err(|error| error.to_string())?;
        let state = store.load_state().map_err(|error| error.to_string())?;
        let configured_gh_path = store
            .gh_executable_path()
            .map_err(|error| error.to_string())?;
        let gh_executable_path = resolve_gh_executable(configured_gh_path.as_deref()).ok();
        if gh_executable_path != configured_gh_path {
            if let Some(path) = gh_executable_path.as_deref() {
                store
                    .set_gh_executable_path(path)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(Self {
            store,
            state,
            gh_executable_path,
        })
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

    fn link_external_object(
        &mut self,
        item_id: i64,
        url: String,
    ) -> Result<ExternalLinkAction, String> {
        let object_input = classify_url(&url).map_err(|error| error.to_string())?;
        let known_object = self.state.external_objects.iter().find(|object| {
            object.provider == object_input.provider
                && object.external_key == object_input.external_key
        });
        let mut warning = None;
        let snapshot =
            if object_input.provider == ExternalProvider::GitHub && known_object.is_none() {
                match self.fetch_github_object(&object_input) {
                    Ok(snapshot) => Some(snapshot),
                    Err(error) => {
                        warning = Some(error);
                        None
                    }
                }
            } else {
                None
            };
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id,
                object: object_input,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| "Link creation produced no Link".to_owned())?;
        self.commit(decision)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| "Link creation produced no External Object".to_owned())?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    fn refresh_external_object(
        &mut self,
        external_object_id: i64,
    ) -> Result<ExternalSnapshot, String> {
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == external_object_id)
            .cloned()
            .ok_or_else(|| format!("External Object {external_object_id} does not exist"))?;
        if object.provider != ExternalProvider::GitHub {
            return Err("Only GitHub External Objects can be refreshed".into());
        }
        let input = ExternalObjectInput {
            provider: object.provider,
            kind: object.kind,
            external_key: object.external_key.clone(),
            canonical_url: object.canonical_url.clone(),
        };
        let snapshot = self.fetch_github_object(&input)?;
        let decision = decide(
            self.state.clone(),
            Event::RefreshExternalObject {
                external_object_id,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let snapshot = decision
            .state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == external_object_id)
            .cloned()
            .ok_or_else(|| "Refresh produced no External snapshot".to_owned())?;
        self.commit(decision)?;
        Ok(snapshot)
    }

    fn fetch_github_object(
        &mut self,
        object: &ExternalObjectInput,
    ) -> Result<crate::domain::ExternalSnapshotData, String> {
        let executable = self.gh_executable_path()?;
        GithubCli::new(executable)
            .fetch(object, current_unix_seconds())
            .map_err(|error| error.to_string())
    }

    fn gh_executable_path(&mut self) -> Result<PathBuf, String> {
        let stored_path = self.gh_executable_path.as_deref();
        let executable = resolve_gh_executable(stored_path).map_err(|error| error.to_string())?;
        if self.gh_executable_path.as_deref() != Some(executable.as_path()) {
            self.store
                .set_gh_executable_path(&executable)
                .map_err(|error| error.to_string())?;
            self.gh_executable_path = Some(executable.clone());
        }
        Ok(executable)
    }

    fn commit(&mut self, decision: crate::domain::Decision) -> Result<(), String> {
        self.store
            .apply(&decision.effects)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExternalLinkAction {
    pub link: ExternalLinkView,
    pub warning: Option<String>,
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
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

#[tauri::command(rename_all = "camelCase")]
pub fn link_external_object(
    item_id: i64,
    url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .link_external_object(item_id, url)
}

#[tauri::command(rename_all = "camelCase")]
pub fn refresh_external_object(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalSnapshot, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .refresh_external_object(external_object_id)
}
