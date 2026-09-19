use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDefaults {
    pub item_status: ItemStatus,
}

impl Default for ProjectDefaults {
    fn default() -> Self {
        Self {
            item_status: ItemStatus::Inbox,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub context_id: i64,
    pub name: String,
    pub defaults: ProjectDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub remind_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub project_id: i64,
    pub status: ItemStatus,
    pub notes: String,
    pub reminders: Vec<Reminder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalProvider {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "generic")]
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalObjectKind {
    #[serde(rename = "issue")]
    Issue,
    #[serde(rename = "pull_request")]
    PullRequest,
    #[serde(rename = "generic")]
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalObjectInput {
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalObject {
    pub id: i64,
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMetadata {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSnapshotData {
    pub title: String,
    pub state: String,
    pub metadata: Vec<ExternalMetadata>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSnapshot {
    pub external_object_id: i64,
    pub title: String,
    pub state: String,
    pub metadata: Vec<ExternalMetadata>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalChangeKind {
    #[serde(rename = "title")]
    Title,
    #[serde(rename = "state")]
    State,
    #[serde(rename = "metadata")]
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChange {
    pub kind: ExternalChangeKind,
    pub key: Option<String>,
    pub previous: Option<String>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub id: i64,
    pub external_object_id: i64,
    pub observed_at: i64,
    pub changes: Vec<ExternalChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChangePolicy {
    pub title: bool,
    pub state: bool,
    pub metadata: bool,
}

impl ExternalChangePolicy {
    pub const fn all() -> Self {
        Self {
            title: true,
            state: true,
            metadata: true,
        }
    }

    fn allows(self, kind: ExternalChangeKind) -> bool {
        match kind {
            ExternalChangeKind::Title => self.title,
            ExternalChangeKind::State => self.state,
            ExternalChangeKind::Metadata => self.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAttentionDefault {
    pub context_id: i64,
    pub object_kind: ExternalObjectKind,
    pub policy: ExternalChangePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub id: i64,
    pub item_id: i64,
    pub external_object_id: i64,
    pub reviewed_activity_id: i64,
    pub attention_policy: Option<ExternalChangePolicy>,
    pub watch_until: Option<String>,
    pub review_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttentionEntryKind {
    #[serde(rename = "external_change")]
    ExternalChange,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "reminder")]
    Reminder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionEntry {
    pub kind: AttentionEntryKind,
    pub link_id: i64,
    pub reminder_id: Option<i64>,
    pub item_id: i64,
    pub external_object_id: i64,
    pub source_title: String,
    pub source_url: String,
    pub activities: Vec<Activity>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLinkView {
    pub link: Link,
    pub object: ExternalObject,
    pub snapshot: Option<ExternalSnapshot>,
    pub attention_policy: ExternalChangePolicy,
    pub attention_entry: Option<AttentionEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Inbox,
    Active,
    Waiting,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemRelationKind {
    Blocks,
    BlockedBy,
    RelatedTo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRelation {
    pub from_item_id: i64,
    pub to_item_id: i64,
    pub kind: ItemRelationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    pub item: Item,
    pub context_name: String,
    pub project_name: String,
    pub relationships: Vec<ItemRelation>,
    pub links: Vec<ExternalLinkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeView {
    pub needs_attention: Vec<ItemView>,
    pub attention_entries: Vec<AttentionEntry>,
    pub running: Vec<ItemView>,
    pub waiting: Vec<ItemView>,
    pub due: Vec<ItemView>,
    pub completed: Vec<ItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainState {
    pub next_context_id: i64,
    pub next_project_id: i64,
    pub next_item_id: i64,
    pub next_item_number: i64,
    pub next_external_object_id: i64,
    pub next_link_id: i64,
    pub next_activity_id: i64,
    pub next_reminder_id: i64,
    pub contexts: Vec<Context>,
    pub projects: Vec<Project>,
    pub items: Vec<Item>,
    pub relationships: Vec<ItemRelation>,
    pub external_objects: Vec<ExternalObject>,
    pub links: Vec<Link>,
    pub snapshots: Vec<ExternalSnapshot>,
    pub activities: Vec<Activity>,
    pub attention_defaults: Vec<ContextAttentionDefault>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    CreateContext {
        name: String,
    },
    CreateProject {
        context_id: i64,
        name: String,
        defaults: ProjectDefaults,
    },
    CreateItem {
        title: String,
        context_id: i64,
        project_id: i64,
    },
    SetItemStatus {
        item_id: i64,
        status: ItemStatus,
    },
    SetItemNotes {
        item_id: i64,
        notes: String,
    },
    SetItemRelation {
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    },
    AddItemReminder {
        item_id: i64,
        remind_at: String,
    },
    RemoveItemReminder {
        item_id: i64,
        reminder_id: i64,
    },
    SetLinkWatchUntil {
        link_id: i64,
        watch_until: Option<String>,
    },
    SetLinkReviewAt {
        link_id: i64,
        review_at: Option<String>,
    },
    ClearLinkReviewAt {
        link_id: i64,
    },
    LinkExternalObject {
        item_id: i64,
        object: ExternalObjectInput,
        snapshot: Option<ExternalSnapshotData>,
    },
    RefreshExternalObject {
        external_object_id: i64,
        snapshot: ExternalSnapshotData,
    },
    SetLinkAttentionPolicy {
        link_id: i64,
        policy: Option<ExternalChangePolicy>,
    },
    SetContextAttentionDefault {
        context_id: i64,
        object_kind: ExternalObjectKind,
        policy: ExternalChangePolicy,
    },
    MarkLinkReviewed {
        link_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    PersistContext {
        context: Context,
        next_context_id: i64,
    },
    PersistProject {
        project: Project,
        next_project_id: i64,
    },
    PersistItem {
        item: Item,
        next_item_number: i64,
        next_item_id: i64,
    },
    PersistItemUpdate {
        item: Item,
    },
    PersistItemReminders {
        item: Item,
        next_reminder_id: i64,
    },
    PersistItemRelation {
        relation: ItemRelation,
    },
    PersistExternalObject {
        object: ExternalObject,
        next_external_object_id: i64,
    },
    PersistLink {
        link: Link,
        next_link_id: i64,
    },
    PersistLinkState {
        link: Link,
    },
    PersistExternalSnapshot {
        snapshot: ExternalSnapshot,
    },
    PersistActivity {
        activity: Activity,
        next_activity_id: i64,
    },
    PersistContextAttentionDefault {
        attention_default: ContextAttentionDefault,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub state: DomainState,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("a Context name cannot be blank")]
    EmptyContextName,
    #[error("a Project name cannot be blank")]
    EmptyProjectName,
    #[error("an Item title cannot be blank")]
    EmptyTitle,
    #[error("a Reminder date cannot be blank")]
    EmptyReminderAt,
    #[error("Context name already exists: {name}")]
    ContextNameTaken { name: String },
    #[error("Project name already exists in Context {context_id}: {name}")]
    ProjectNameTaken { context_id: i64, name: String },
    #[error("Context {context_id} does not exist")]
    ContextNotFound { context_id: i64 },
    #[error("Project {project_id} does not exist")]
    ProjectNotFound { project_id: i64 },
    #[error("Project {project_id} belongs to another Context")]
    ProjectContextMismatch { project_id: i64, context_id: i64 },
    #[error("Item {item_id} does not exist")]
    ItemNotFound { item_id: i64 },
    #[error("Items {from_item_id} and {to_item_id} belong to different Contexts")]
    ItemContextMismatch { from_item_id: i64, to_item_id: i64 },
    #[error("an Item cannot relate to itself: {item_id}")]
    SelfRelation { item_id: i64 },
    #[error("the relationship already exists")]
    RelationAlreadyExists,
    #[error("the Item identifier sequence is exhausted")]
    SequenceExhausted,
    #[error("an external URL cannot be blank")]
    EmptyExternalUrl,
    #[error("an external object key cannot be blank")]
    EmptyExternalObjectKey,
    #[error("External Object {external_object_id} does not exist")]
    ExternalObjectNotFound { external_object_id: i64 },
    #[error("the Link already exists")]
    LinkAlreadyExists,
    #[error("Link {link_id} does not exist")]
    LinkNotFound { link_id: i64 },
    #[error("Reminder {reminder_id} does not exist on Item {item_id}")]
    ReminderNotFound { item_id: i64, reminder_id: i64 },
}

pub fn decide(mut state: DomainState, event: Event) -> Result<Decision, DomainError> {
    match event {
        Event::CreateContext { name } => {
            let name = clean_name(name, DomainError::EmptyContextName)?;
            if state.contexts.iter().any(|context| context.name == name) {
                return Err(DomainError::ContextNameTaken { name });
            }

            let id = state.next_context_id;
            let next_context_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context { id, name };
            let project = Project {
                id: project_id,
                context_id: id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts.push(context.clone());
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistContext {
                        context,
                        next_context_id,
                    },
                    Effect::PersistProject {
                        project,
                        next_project_id,
                    },
                ],
            })
        }
        Event::CreateProject {
            context_id,
            name,
            defaults,
        } => {
            let name = clean_name(name, DomainError::EmptyProjectName)?;
            ensure_context(&state, context_id)?;
            if state
                .projects
                .iter()
                .any(|project| project.context_id == context_id && project.name == name)
            {
                return Err(DomainError::ProjectNameTaken { context_id, name });
            }

            let id = state.next_project_id;
            let next_project_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project = Project {
                id,
                context_id,
                name,
                defaults,
            };

            state.next_project_id = next_project_id;
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistProject {
                    project,
                    next_project_id,
                }],
            })
        }
        Event::CreateItem {
            title,
            context_id,
            project_id,
        } => {
            if title.trim().is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            ensure_context(&state, context_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if project.context_id != context_id {
                return Err(DomainError::ProjectContextMismatch {
                    project_id,
                    context_id,
                });
            }

            let id = state.next_item_id;
            let number = state.next_item_number;
            let next_item_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let next_item_number = number
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let item = Item {
                id,
                human_identifier: format!("MC-{number}"),
                title: title.trim().to_owned(),
                project_id,
                status: project.defaults.item_status,
                notes: String::new(),
                reminders: Vec::new(),
            };

            state.next_item_id = next_item_id;
            state.next_item_number = next_item_number;
            state.items.push(item.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItem {
                    item,
                    next_item_number,
                    next_item_id,
                }],
            })
        }
        Event::SetItemStatus { item_id, status } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.status = status;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemNotes { item_id, notes } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.notes = notes;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemRelation {
            from_item_id,
            to_item_id,
            kind,
        } => {
            if from_item_id == to_item_id {
                return Err(DomainError::SelfRelation {
                    item_id: from_item_id,
                });
            }
            let from_context_id = item_context_id(&state, from_item_id)?;
            let to_context_id = item_context_id(&state, to_item_id)?;
            if from_context_id != to_context_id {
                return Err(DomainError::ItemContextMismatch {
                    from_item_id,
                    to_item_id,
                });
            }

            let relation = ItemRelation {
                from_item_id,
                to_item_id,
                kind,
            };
            if state.relationships.contains(&relation) {
                return Err(DomainError::RelationAlreadyExists);
            }
            state.relationships.push(relation.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemRelation { relation }],
            })
        }
        Event::AddItemReminder { item_id, remind_at } => {
            let remind_at = clean_name(remind_at, DomainError::EmptyReminderAt)?;
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let id = state.next_reminder_id;
            let next_reminder_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            item.reminders.push(Reminder { id, remind_at });
            state.next_reminder_id = next_reminder_id;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::RemoveItemReminder {
            item_id,
            reminder_id,
        } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let position = item
                .reminders
                .iter()
                .position(|reminder| reminder.id == reminder_id)
                .ok_or(DomainError::ReminderNotFound {
                    item_id,
                    reminder_id,
                })?;
            item.reminders.remove(position);
            let item = item.clone();
            let next_reminder_id = state.next_reminder_id;

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::SetLinkWatchUntil {
            link_id,
            watch_until,
        } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.watch_until = watch_until;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetLinkReviewAt { link_id, review_at } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = review_at;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::ClearLinkReviewAt { link_id } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = None;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::LinkExternalObject {
            item_id,
            object,
            snapshot,
        } => {
            ensure_item(&state, item_id)?;
            if object.canonical_url.trim().is_empty() {
                return Err(DomainError::EmptyExternalUrl);
            }
            if object.external_key.trim().is_empty() {
                return Err(DomainError::EmptyExternalObjectKey);
            }

            let existing_object = state
                .external_objects
                .iter()
                .find(|candidate| {
                    candidate.provider == object.provider
                        && candidate.external_key == object.external_key
                })
                .cloned();
            let (external_object, is_new_object) = match existing_object {
                Some(object) => (object, false),
                None => {
                    let id = state.next_external_object_id;
                    let next_external_object_id =
                        id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                    let object = ExternalObject {
                        id,
                        provider: object.provider,
                        kind: object.kind,
                        external_key: object.external_key,
                        canonical_url: object.canonical_url.trim().to_owned(),
                    };
                    state.next_external_object_id = next_external_object_id;
                    state.external_objects.push(object.clone());
                    (object, true)
                }
            };

            if state.links.iter().any(|link| {
                link.item_id == item_id && link.external_object_id == external_object.id
            }) {
                return Err(DomainError::LinkAlreadyExists);
            }

            let link_id = state.next_link_id;
            let next_link_id = link_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let reviewed_activity_id = state
                .activities
                .iter()
                .filter(|activity| activity.external_object_id == external_object.id)
                .map(|activity| activity.id)
                .max()
                .unwrap_or_default();
            let link = Link {
                id: link_id,
                item_id,
                external_object_id: external_object.id,
                reviewed_activity_id,
                attention_policy: None,
                watch_until: None,
                review_at: None,
            };
            state.next_link_id = next_link_id;
            state.links.push(link.clone());

            let mut effects = Vec::new();
            if is_new_object {
                effects.push(Effect::PersistExternalObject {
                    object: external_object.clone(),
                    next_external_object_id: state.next_external_object_id,
                });
            }
            effects.push(Effect::PersistLink { link, next_link_id });
            if let Some(snapshot_data) = snapshot {
                let snapshot = ExternalSnapshot {
                    external_object_id: external_object.id,
                    title: snapshot_data.title,
                    state: snapshot_data.state,
                    metadata: snapshot_data.metadata,
                    fetched_at: snapshot_data.fetched_at,
                };
                upsert_snapshot(&mut state, snapshot.clone());
                effects.push(Effect::PersistExternalSnapshot { snapshot });
            }

            Ok(Decision { state, effects })
        }
        Event::RefreshExternalObject {
            external_object_id,
            snapshot: snapshot_data,
        } => {
            if !state
                .external_objects
                .iter()
                .any(|object| object.id == external_object_id)
            {
                return Err(DomainError::ExternalObjectNotFound { external_object_id });
            }
            let snapshot = ExternalSnapshot {
                external_object_id,
                title: snapshot_data.title,
                state: snapshot_data.state,
                metadata: snapshot_data.metadata,
                fetched_at: snapshot_data.fetched_at,
            };
            let changes = state
                .snapshots
                .iter()
                .find(|existing| existing.external_object_id == external_object_id)
                .map(|previous| snapshot_changes(previous, &snapshot))
                .unwrap_or_default();
            upsert_snapshot(&mut state, snapshot.clone());

            let mut effects = Vec::new();
            if !changes.is_empty() {
                let id = state.next_activity_id;
                let next_activity_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                let activity = Activity {
                    id,
                    external_object_id,
                    observed_at: snapshot.fetched_at,
                    changes,
                };
                state.next_activity_id = next_activity_id;
                state.activities.push(activity.clone());
                effects.push(Effect::PersistActivity {
                    activity,
                    next_activity_id,
                });
            }
            effects.push(Effect::PersistExternalSnapshot { snapshot });

            Ok(Decision { state, effects })
        }
        Event::SetLinkAttentionPolicy { link_id, policy } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.attention_policy = policy;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetContextAttentionDefault {
            context_id,
            object_kind,
            policy,
        } => {
            ensure_context(&state, context_id)?;
            let attention_default = ContextAttentionDefault {
                context_id,
                object_kind,
                policy,
            };
            if let Some(existing) = state.attention_defaults.iter_mut().find(|existing| {
                existing.context_id == context_id && existing.object_kind == object_kind
            }) {
                *existing = attention_default.clone();
            } else {
                state.attention_defaults.push(attention_default.clone());
            }

            Ok(Decision {
                state,
                effects: vec![Effect::PersistContextAttentionDefault { attention_default }],
            })
        }
        Event::MarkLinkReviewed { link_id } => {
            let external_object_id = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?
                .external_object_id;
            let reviewed_activity_id = state
                .activities
                .iter()
                .filter(|activity| activity.external_object_id == external_object_id)
                .map(|activity| activity.id)
                .max()
                .unwrap_or_default();
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .expect("the Link was checked above");
            link.reviewed_activity_id = reviewed_activity_id;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
    }
}

pub fn home_view(state: &DomainState, context_id: Option<i64>, now: &str) -> HomeView {
    let mut view = HomeView {
        needs_attention: Vec::new(),
        attention_entries: attention_entries(state, context_id, now),
        running: Vec::new(),
        waiting: Vec::new(),
        due: Vec::new(),
        completed: Vec::new(),
    };

    for item in item_views_at(state, context_id, Some(now)) {
        let is_due = item_has_due_reminder(&item.item, now) && item.item.status != ItemStatus::Done;
        if is_due {
            view.due.push(item.clone());
        }
        if is_due
            || item.item.status == ItemStatus::Inbox
            || (item.item.status != ItemStatus::Done
                && view
                    .attention_entries
                    .iter()
                    .any(|entry| entry.item_id == item.item.id))
        {
            view.needs_attention.push(item.clone());
        }
        match item.item.status {
            ItemStatus::Inbox => {}
            ItemStatus::Active => view.running.push(item),
            ItemStatus::Waiting => view.waiting.push(item),
            ItemStatus::Done => view.completed.push(item),
        }
    }

    view
}

pub fn search_items(state: &DomainState, query: &str, context_id: Option<i64>) -> Vec<ItemView> {
    let query = query.trim().to_lowercase();
    item_views(state, context_id)
        .into_iter()
        .filter(|view| {
            query.is_empty()
                || [
                    view.item.human_identifier.as_str(),
                    view.item.title.as_str(),
                    view.item.notes.as_str(),
                    view.context_name.as_str(),
                    view.project_name.as_str(),
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
        })
        .collect()
}

pub fn external_link_view(state: &DomainState, link: &Link) -> Option<ExternalLinkView> {
    external_link_view_at(state, link, None)
}

fn external_link_view_at(
    state: &DomainState,
    link: &Link,
    now: Option<&str>,
) -> Option<ExternalLinkView> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == link.external_object_id)?;
    let snapshot = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .cloned();
    Some(ExternalLinkView {
        link: link.clone(),
        object: object.clone(),
        snapshot,
        attention_policy: effective_attention_policy(state, link, object),
        attention_entry: attention_entry_for_link_at(state, link, object, now),
    })
}

pub fn attention_entries(
    state: &DomainState,
    context_id: Option<i64>,
    now: &str,
) -> Vec<AttentionEntry> {
    attention_entries_at(state, context_id, Some(now))
}

fn attention_entries_at(
    state: &DomainState,
    context_id: Option<i64>,
    now: Option<&str>,
) -> Vec<AttentionEntry> {
    let mut entries = Vec::new();
    for link in state.links.iter().filter(|link| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, link.item_id)
                .map(|link_context_id| link_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id);
        let Some(object) = object else {
            continue;
        };
        if let Some(entry) = attention_entry_for_link_at(state, link, object, now) {
            entries.push(entry);
        }
        if link
            .review_at
            .as_deref()
            .is_some_and(|review_at| now.is_some_and(|now| review_at <= now))
        {
            entries.push(AttentionEntry {
                kind: AttentionEntryKind::Review,
                link_id: link.id,
                reminder_id: None,
                item_id: link.item_id,
                external_object_id: object.id,
                source_title: state
                    .snapshots
                    .iter()
                    .find(|snapshot| snapshot.external_object_id == object.id)
                    .map(|snapshot| snapshot.title.clone())
                    .unwrap_or_else(|| object.canonical_url.clone()),
                source_url: object.canonical_url.clone(),
                activities: Vec::new(),
                summary: format!(
                    "Review scheduled for {}",
                    link.review_at.as_deref().unwrap_or_default()
                ),
            });
        }
    }

    for item in state.items.iter().filter(|item| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, item.id)
                .map(|item_context_id| item_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        entries.extend(
            item.reminders
                .iter()
                .filter(|reminder| reminder.remind_at.as_str() <= now.unwrap_or_default())
                .map(|reminder| AttentionEntry {
                    kind: AttentionEntryKind::Reminder,
                    link_id: 0,
                    reminder_id: Some(reminder.id),
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Reminder due at {}", reminder.remind_at),
                }),
        );
    }

    entries
}

fn item_has_due_reminder(item: &Item, now: &str) -> bool {
    item.reminders
        .iter()
        .any(|reminder| reminder.remind_at.as_str() <= now)
}

fn item_views(state: &DomainState, context_id: Option<i64>) -> Vec<ItemView> {
    item_views_at(state, context_id, None)
}

fn item_views_at(state: &DomainState, context_id: Option<i64>, now: Option<&str>) -> Vec<ItemView> {
    state
        .items
        .iter()
        .filter_map(|item| {
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            if context_id.is_some_and(|candidate| candidate != project.context_id) {
                return None;
            }
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;
            let relationships = state
                .relationships
                .iter()
                .filter(|relation| {
                    relation.from_item_id == item.id || relation.to_item_id == item.id
                })
                .cloned()
                .collect();
            let links = state
                .links
                .iter()
                .filter(|link| link.item_id == item.id)
                .filter_map(|link| external_link_view_at(state, link, now))
                .collect();
            Some(ItemView {
                item: item.clone(),
                context_name: context.name.clone(),
                project_name: project.name.clone(),
                relationships,
                links,
            })
        })
        .collect()
}

fn clean_name<E>(name: String, empty_error: E) -> Result<String, E> {
    let name = name.trim();
    if name.is_empty() {
        return Err(empty_error);
    }
    Ok(name.to_owned())
}

fn ensure_context(state: &DomainState, context_id: i64) -> Result<(), DomainError> {
    if state
        .contexts
        .iter()
        .any(|context| context.id == context_id)
    {
        Ok(())
    } else {
        Err(DomainError::ContextNotFound { context_id })
    }
}

fn item_context_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.context_id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

fn ensure_item(state: &DomainState, item_id: i64) -> Result<(), DomainError> {
    if state.items.iter().any(|item| item.id == item_id) {
        Ok(())
    } else {
        Err(DomainError::ItemNotFound { item_id })
    }
}

fn upsert_snapshot(state: &mut DomainState, snapshot: ExternalSnapshot) {
    if let Some(existing) = state
        .snapshots
        .iter_mut()
        .find(|existing| existing.external_object_id == snapshot.external_object_id)
    {
        *existing = snapshot;
    } else {
        state.snapshots.push(snapshot);
    }
}

fn effective_attention_policy(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
) -> ExternalChangePolicy {
    if let Some(policy) = link.attention_policy {
        return policy;
    }
    let context_id = item_context_id(state, link.item_id).ok();
    context_id
        .and_then(|context_id| {
            state.attention_defaults.iter().find(|attention_default| {
                attention_default.context_id == context_id
                    && attention_default.object_kind == object.kind
            })
        })
        .map(|attention_default| attention_default.policy)
        .unwrap_or_else(ExternalChangePolicy::all)
}

fn attention_entry_for_link_at(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
    now: Option<&str>,
) -> Option<AttentionEntry> {
    let policy = effective_attention_policy(state, link, object);
    let watch_active = now.is_none_or(|now| {
        link.watch_until
            .as_deref()
            .is_none_or(|watch_until| watch_until > now)
    });
    let activities = state
        .activities
        .iter()
        .filter(|activity| {
            watch_active
                && activity.external_object_id == object.id
                && activity.id > link.reviewed_activity_id
        })
        .filter_map(|activity| {
            let changes = activity
                .changes
                .iter()
                .filter(|change| policy.allows(change.kind))
                .cloned()
                .collect::<Vec<_>>();
            (!changes.is_empty()).then_some(Activity {
                id: activity.id,
                external_object_id: activity.external_object_id,
                observed_at: activity.observed_at,
                changes,
            })
        })
        .collect::<Vec<_>>();
    if activities.is_empty() {
        return None;
    }

    let source_title = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .map(|snapshot| snapshot.title.clone())
        .unwrap_or_else(|| object.canonical_url.clone());
    let summary = activities
        .iter()
        .flat_map(|activity| activity.changes.iter())
        .map(format_change)
        .collect::<Vec<_>>()
        .join("; ");

    Some(AttentionEntry {
        kind: AttentionEntryKind::ExternalChange,
        link_id: link.id,
        reminder_id: None,
        item_id: link.item_id,
        external_object_id: object.id,
        source_title,
        source_url: object.canonical_url.clone(),
        activities,
        summary,
    })
}

fn snapshot_changes(
    previous: &ExternalSnapshot,
    current: &ExternalSnapshot,
) -> Vec<ExternalChange> {
    let mut changes = Vec::new();
    if previous.title != current.title {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::Title,
            key: None,
            previous: Some(previous.title.clone()),
            current: Some(current.title.clone()),
        });
    }
    if previous.state != current.state {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::State,
            key: None,
            previous: Some(previous.state.clone()),
            current: Some(current.state.clone()),
        });
    }

    let mut keys = previous
        .metadata
        .iter()
        .map(|metadata| metadata.key.clone())
        .chain(current.metadata.iter().map(|metadata| metadata.key.clone()))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    for key in keys {
        let previous_value = previous
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        let current_value = current
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        if previous_value != current_value {
            changes.push(ExternalChange {
                kind: ExternalChangeKind::Metadata,
                key: Some(key),
                previous: previous_value,
                current: current_value,
            });
        }
    }
    changes
}

fn format_change(change: &ExternalChange) -> String {
    let label = match change.kind {
        ExternalChangeKind::Title => "Title".to_owned(),
        ExternalChangeKind::State => "State".to_owned(),
        ExternalChangeKind::Metadata => {
            format!("Metadata {}", change.key.as_deref().unwrap_or("value"))
        }
    };
    match (&change.previous, &change.current) {
        (Some(previous), Some(current)) => format!("{label} changed from {previous} to {current}"),
        (None, Some(current)) => format!("{label} added as {current}"),
        (Some(previous), None) => format!("{label} removed (was {previous})"),
        (None, None) => format!("{label} changed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_a_context_creates_its_default_project() {
        let decision = decide(
            empty_state(),
            Event::CreateContext {
                name: "Work".into(),
            },
        )
        .expect("Context creation should succeed");

        assert_eq!(
            decision.state.contexts,
            vec![Context {
                id: 1,
                name: "Work".into(),
            }]
        );
        assert_eq!(
            decision.state.projects,
            vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Inbox,
                },
            }]
        );
        assert_eq!(
            decision.effects,
            vec![
                Effect::PersistContext {
                    context: decision.state.contexts[0].clone(),
                    next_context_id: 2,
                },
                Effect::PersistProject {
                    project: decision.state.projects[0].clone(),
                    next_project_id: 2,
                },
            ]
        );
    }

    #[test]
    fn creating_a_project_keeps_its_item_defaults() {
        let decision = decide(
            state_with_context(7, "Work"),
            Event::CreateProject {
                context_id: 7,
                name: "Billing".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Active,
                },
            },
        )
        .expect("Project creation should succeed");

        assert_eq!(
            decision.state.projects[1],
            Project {
                id: 2,
                context_id: 7,
                name: "Billing".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Active,
                },
            }
        );
    }

    #[test]
    fn creating_an_item_assigns_the_project_and_inherits_its_defaults() {
        let mut state = state_with_context(7, "Work");
        state.projects.push(Project {
            id: 2,
            context_id: 7,
            name: "Billing".into(),
            defaults: ProjectDefaults {
                item_status: ItemStatus::Active,
            },
        });
        state.next_project_id = 3;

        let decision = decide(
            state,
            Event::CreateItem {
                title: "Investigate timeout".into(),
                context_id: 7,
                project_id: 2,
            },
        )
        .expect("item creation should succeed");

        assert_eq!(decision.state.items.len(), 1);
        assert_eq!(
            decision.state.items[0],
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Investigate timeout".into(),
                project_id: 2,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }
        );
        assert_eq!(decision.state.next_item_number, 2);
        assert_eq!(decision.state.next_item_id, 2);
        assert_eq!(
            decision.effects,
            vec![Effect::PersistItem {
                item: decision.state.items[0].clone(),
                next_item_number: 2,
                next_item_id: 2,
            }]
        );
    }

    #[test]
    fn item_identifiers_use_one_global_sequence_across_projects() {
        let state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);

        let first = decide(
            state,
            Event::CreateItem {
                title: "Work item".into(),
                context_id: 7,
                project_id: 1,
            },
        )
        .expect("first item should succeed");
        let second = decide(
            first.state,
            Event::CreateItem {
                title: "Personal item".into(),
                context_id: 8,
                project_id: 2,
            },
        )
        .expect("second item should succeed");

        assert_eq!(second.state.items[0].human_identifier, "MC-1");
        assert_eq!(second.state.items[1].human_identifier, "MC-2");
    }

    #[test]
    fn an_action_cannot_cross_contexts() {
        let state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);

        assert_eq!(
            decide(
                state,
                Event::CreateItem {
                    title: "Wrong boundary".into(),
                    context_id: 7,
                    project_id: 2,
                },
            ),
            Err(DomainError::ProjectContextMismatch {
                project_id: 2,
                context_id: 7,
            })
        );
    }

    #[test]
    fn creation_rejects_blank_names_titles_and_unknown_owners() {
        let state = state_with_context(7, "Work");

        assert_eq!(
            decide(state.clone(), Event::CreateContext { name: "   ".into() },),
            Err(DomainError::EmptyContextName)
        );
        assert_eq!(
            decide(
                state.clone(),
                Event::CreateProject {
                    context_id: 99,
                    name: "No context".into(),
                    defaults: ProjectDefaults {
                        item_status: ItemStatus::Inbox,
                    },
                },
            ),
            Err(DomainError::ContextNotFound { context_id: 99 })
        );
        assert_eq!(
            decide(
                state.clone(),
                Event::CreateItem {
                    title: "   ".into(),
                    context_id: 7,
                    project_id: 1,
                },
            ),
            Err(DomainError::EmptyTitle)
        );
        assert_eq!(
            decide(
                state.clone(),
                Event::CreateItem {
                    title: "No context".into(),
                    context_id: 99,
                    project_id: 1,
                },
            ),
            Err(DomainError::ContextNotFound { context_id: 99 })
        );
        assert_eq!(
            decide(
                state,
                Event::CreateItem {
                    title: "No project".into(),
                    context_id: 7,
                    project_id: 99,
                },
            ),
            Err(DomainError::ProjectNotFound { project_id: 99 })
        );
    }

    #[test]
    fn an_item_can_move_between_statuses_in_any_order() {
        let mut state = state_with_context(7, "Work");
        state.items.push(Item {
            id: 1,
            human_identifier: "MC-1".into(),
            title: "Ship the change".into(),
            project_id: 1,
            status: ItemStatus::Inbox,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.next_item_id = 2;
        state.next_item_number = 2;

        let active = decide(
            state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Active,
            },
        )
        .expect("an Item should move to Active");
        assert_eq!(active.state.items[0].status, ItemStatus::Active);
        assert_eq!(
            active.effects,
            vec![Effect::PersistItemUpdate {
                item: active.state.items[0].clone(),
            }]
        );

        let waiting = decide(
            active.state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Waiting,
            },
        )
        .expect("an Item should move to Waiting");
        assert_eq!(waiting.state.items[0].status, ItemStatus::Waiting);

        let done = decide(
            waiting.state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Done,
            },
        )
        .expect("an Item should move to Done");
        let inbox = decide(
            done.state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Inbox,
            },
        )
        .expect("a Done Item should be able to return to Inbox");
        assert_eq!(inbox.state.items[0].status, ItemStatus::Inbox);
    }

    #[test]
    fn an_item_can_record_notes_and_relationships_within_its_context() {
        let mut state = state_with_context(7, "Work");
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Ship the change".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Prepare the release".into(),
                project_id: 1,
                status: ItemStatus::Waiting,
                notes: String::new(),
                reminders: Vec::new(),
            },
        ];
        state.next_item_id = 3;
        state.next_item_number = 3;

        let noted = decide(
            state,
            Event::SetItemNotes {
                item_id: 1,
                notes: "Release after the migration is verified.".into(),
            },
        )
        .expect("notes should be saved");
        assert_eq!(
            noted.state.items[0].notes,
            "Release after the migration is verified."
        );
        assert_eq!(
            noted.effects,
            vec![Effect::PersistItemUpdate {
                item: noted.state.items[0].clone(),
            }]
        );

        let related = decide(
            noted.state,
            Event::SetItemRelation {
                from_item_id: 1,
                to_item_id: 2,
                kind: ItemRelationKind::Blocks,
            },
        )
        .expect("Items in one Context should be related");
        assert_eq!(
            related.state.relationships,
            vec![ItemRelation {
                from_item_id: 1,
                to_item_id: 2,
                kind: ItemRelationKind::Blocks,
            }]
        );
        assert_eq!(
            related.effects,
            vec![Effect::PersistItemRelation {
                relation: related.state.relationships[0].clone(),
            }]
        );
    }

    #[test]
    fn relationships_cannot_cross_contexts_or_point_to_themselves() {
        let mut state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Work item".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Personal item".into(),
                project_id: 2,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
        ];

        assert_eq!(
            decide(
                state.clone(),
                Event::SetItemRelation {
                    from_item_id: 1,
                    to_item_id: 2,
                    kind: ItemRelationKind::BlockedBy,
                },
            ),
            Err(DomainError::ItemContextMismatch {
                from_item_id: 1,
                to_item_id: 2,
            })
        );
        assert_eq!(
            decide(
                state,
                Event::SetItemRelation {
                    from_item_id: 1,
                    to_item_id: 1,
                    kind: ItemRelationKind::RelatedTo,
                },
            ),
            Err(DomainError::SelfRelation { item_id: 1 })
        );
    }

    #[test]
    fn home_view_groups_items_and_searches_across_contexts() {
        let mut state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Reply to the design review".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: "Needs a decision".into(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Implement the parser".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: vec![Reminder {
                    id: 1,
                    remind_at: "2026-09-18T09:00".into(),
                }],
            },
            Item {
                id: 3,
                human_identifier: "MC-3".into(),
                title: "Wait for approval".into(),
                project_id: 2,
                status: ItemStatus::Waiting,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 4,
                human_identifier: "MC-4".into(),
                title: "Archive the old plan".into(),
                project_id: 2,
                status: ItemStatus::Done,
                notes: String::new(),
                reminders: vec![Reminder {
                    id: 2,
                    remind_at: "2026-09-17T09:00".into(),
                }],
            },
        ];

        let view = home_view(&state, None, "2026-09-19T09:00");
        assert_eq!(view.needs_attention[0].item.id, 1);
        assert_eq!(view.needs_attention[1].item.id, 2);
        assert_eq!(view.running[0].item.id, 2);
        assert_eq!(view.waiting[0].item.id, 3);
        assert_eq!(view.due[0].item.id, 2);
        assert_eq!(view.completed[0].item.id, 4);
        assert_eq!(view.attention_entries.len(), 2);
        assert!(view
            .attention_entries
            .iter()
            .all(|entry| entry.kind == AttentionEntryKind::Reminder));
        assert_eq!(view.running[0].context_name, "Work");

        let personal = home_view(&state, Some(8), "2026-09-19T09:00");
        assert!(personal.needs_attention.is_empty());
        assert_eq!(personal.attention_entries.len(), 1);
        assert_eq!(personal.waiting[0].context_name, "Personal");

        let results = search_items(&state, "design", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.human_identifier, "MC-1");
        assert_eq!(results[0].context_name, "Work");

        let all_personal = search_items(&state, "", Some(8));
        assert_eq!(all_personal.len(), 2);
        assert!(all_personal
            .iter()
            .all(|result| result.context_name == "Personal"));
    }

    #[test]
    fn linking_a_github_object_creates_one_object_and_one_link_with_a_snapshot() {
        let state = state_with_item(7, "Work");
        let decision = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::PullRequest,
                    external_key: "pull:acme/app#42".into(),
                    canonical_url: "https://github.com/acme/app/pull/42".into(),
                },
                snapshot: Some(ExternalSnapshotData {
                    title: "Ship the parser".into(),
                    state: "OPEN".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 100,
                }),
            },
        )
        .expect("a GitHub object should link to an Item");

        assert_eq!(decision.state.external_objects.len(), 1);
        assert_eq!(decision.state.links.len(), 1);
        assert_eq!(decision.state.snapshots.len(), 1);
        assert_eq!(decision.state.links[0].item_id, 1);
        assert_eq!(decision.state.links[0].external_object_id, 1);
        assert_eq!(decision.state.snapshots[0].title, "Ship the parser");
        let view = search_items(&decision.state, "", None);
        assert_eq!(view[0].links.len(), 1);
        assert_eq!(
            view[0].links[0].snapshot.as_ref().unwrap().title,
            "Ship the parser"
        );
        assert_eq!(
            decision.effects,
            vec![
                Effect::PersistExternalObject {
                    object: decision.state.external_objects[0].clone(),
                    next_external_object_id: 2,
                },
                Effect::PersistLink {
                    link: decision.state.links[0].clone(),
                    next_link_id: 2,
                },
                Effect::PersistExternalSnapshot {
                    snapshot: decision.state.snapshots[0].clone(),
                },
            ]
        );
    }

    #[test]
    fn two_items_share_one_external_object_but_keep_two_links_and_fetch_once() {
        let mut state = state_with_context(7, "Work");
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "First commitment".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Second commitment".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
        ];
        state.next_item_id = 3;
        state.next_item_number = 3;
        let object = ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind: ExternalObjectKind::Issue,
            external_key: "issue:acme/app#7".into(),
            canonical_url: "https://github.com/acme/app/issues/7".into(),
        };

        let first = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: object.clone(),
                snapshot: Some(snapshot_data("Shared issue", 100)),
            },
        )
        .expect("the first Link should succeed");
        let second = decide(
            first.state,
            Event::LinkExternalObject {
                item_id: 2,
                object,
                snapshot: None,
            },
        )
        .expect("the second Link should reuse the object");

        assert_eq!(second.state.external_objects.len(), 1);
        assert_eq!(second.state.links.len(), 2);
        assert_eq!(second.state.snapshots.len(), 1);
        assert_eq!(second.effects.len(), 1);
        assert!(matches!(second.effects[0], Effect::PersistLink { .. }));
    }

    #[test]
    fn an_unrecognised_url_is_stored_as_a_generic_link_and_an_item_can_have_many_links() {
        let state = state_with_item(7, "Work");
        let first = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::Generic,
                    kind: ExternalObjectKind::Generic,
                    external_key: "https://example.com/design".into(),
                    canonical_url: "https://example.com/design".into(),
                },
                snapshot: None,
            },
        )
        .expect("an unrecognised URL should still link");
        let second = decide(
            first.state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::Generic,
                    kind: ExternalObjectKind::Generic,
                    external_key: "https://example.com/spec".into(),
                    canonical_url: "https://example.com/spec".into(),
                },
                snapshot: None,
            },
        )
        .expect("one Item should accept several Links");

        assert_eq!(second.state.external_objects.len(), 2);
        assert_eq!(second.state.links.len(), 2);
        assert!(second.state.snapshots.is_empty());
        assert_eq!(second.state.links[0].item_id, 1);
        assert_eq!(second.state.links[1].item_id, 1);
    }

    #[test]
    fn refreshing_an_external_object_replaces_its_snapshot_without_touching_the_link() {
        let linked = decide(
            state_with_item(7, "Work"),
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "issue:acme/app#7".into(),
                    canonical_url: "https://github.com/acme/app/issues/7".into(),
                },
                snapshot: Some(snapshot_data("Old title", 100)),
            },
        )
        .expect("the Link should exist");
        let refreshed = decide(
            linked.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: snapshot_data("New title", 200),
            },
        )
        .expect("a fresh snapshot should replace the old one");

        assert_eq!(refreshed.state.links.len(), 1);
        assert_eq!(refreshed.state.snapshots[0].title, "New title");
        assert_eq!(refreshed.state.snapshots[0].fetched_at, 200);
        assert_eq!(
            refreshed.effects,
            vec![
                Effect::PersistActivity {
                    activity: refreshed.state.activities[0].clone(),
                    next_activity_id: 2,
                },
                Effect::PersistExternalSnapshot {
                    snapshot: refreshed.state.snapshots[0].clone(),
                },
            ]
        );
    }

    #[test]
    fn refreshing_a_changed_object_records_activity_and_one_attention_entry_per_link() {
        let linked = decide(
            state_with_item(7, "Work"),
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "issue:acme/app#7".into(),
                    canonical_url: "https://github.com/acme/app/issues/7".into(),
                },
                snapshot: Some(ExternalSnapshotData {
                    title: "Old title".into(),
                    state: "OPEN".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 100,
                }),
            },
        )
        .expect("the object should link to the Item");

        let refreshed = decide(
            linked.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: ExternalSnapshotData {
                    title: "New title".into(),
                    state: "CLOSED".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 200,
                },
            },
        )
        .expect("the changed snapshot should be recorded");

        assert_eq!(refreshed.state.activities.len(), 1);
        assert_eq!(refreshed.state.activities[0].changes.len(), 2);
        assert_eq!(
            refreshed.state.activities[0].changes[0].kind,
            ExternalChangeKind::Title
        );
        assert_eq!(
            refreshed.state.activities[0].changes[1].kind,
            ExternalChangeKind::State
        );
        let view = home_view(&refreshed.state, None, "2026-09-20T00:00");
        assert_eq!(view.attention_entries.len(), 1);
        assert_eq!(view.attention_entries[0].activities.len(), 1);
        assert!(view.attention_entries[0].summary.contains("State changed"));
        assert_eq!(
            refreshed.state.links[0].reviewed_activity_id, 0,
            "refreshing must not review the Link"
        );
    }

    #[test]
    fn attention_defaults_and_link_overrides_filter_activity_without_losing_history() {
        let configured = decide(
            state_with_item(7, "Work"),
            Event::SetContextAttentionDefault {
                context_id: 7,
                object_kind: ExternalObjectKind::Issue,
                policy: ExternalChangePolicy {
                    title: false,
                    state: true,
                    metadata: false,
                },
            },
        )
        .expect("the Context default should be configurable");
        let linked = decide(
            configured.state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "issue:acme/app#7".into(),
                    canonical_url: "https://github.com/acme/app/issues/7".into(),
                },
                snapshot: Some(ExternalSnapshotData {
                    title: "Old title".into(),
                    state: "OPEN".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 100,
                }),
            },
        )
        .expect("the object should link to the Item");
        let refreshed = decide(
            linked.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: ExternalSnapshotData {
                    title: "New title".into(),
                    state: "CLOSED".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "someone-else".into(),
                    }],
                    fetched_at: 200,
                },
            },
        )
        .expect("the changed snapshot should be recorded");

        let view = home_view(&refreshed.state, None, "2026-09-20T00:00");
        assert_eq!(view.attention_entries.len(), 1);
        assert_eq!(view.attention_entries[0].activities[0].changes.len(), 1);
        assert_eq!(
            view.attention_entries[0].activities[0].changes[0].kind,
            ExternalChangeKind::State
        );
        assert_eq!(refreshed.state.activities[0].changes.len(), 3);

        let overridden = decide(
            refreshed.state,
            Event::SetLinkAttentionPolicy {
                link_id: 1,
                policy: Some(ExternalChangePolicy {
                    title: true,
                    state: false,
                    metadata: false,
                }),
            },
        )
        .expect("the Link policy should be configurable");
        let overridden_view = home_view(&overridden.state, None, "2026-09-20T00:00");
        assert_eq!(overridden_view.attention_entries.len(), 1);
        assert_eq!(
            overridden_view.attention_entries[0].activities[0].changes[0].kind,
            ExternalChangeKind::Title
        );
    }

    #[test]
    fn marking_one_link_reviewed_leaves_the_other_link_for_the_same_object_unreviewed() {
        let mut state = state_with_item(7, "Work");
        state.items.push(Item {
            id: 2,
            human_identifier: "MC-2".into(),
            title: "Second Item".into(),
            project_id: 1,
            status: ItemStatus::Waiting,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.next_item_id = 3;
        state.next_item_number = 3;
        let object = ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind: ExternalObjectKind::PullRequest,
            external_key: "pull_request:acme/app#7".into(),
            canonical_url: "https://github.com/acme/app/pull/7".into(),
        };
        let first = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: object.clone(),
                snapshot: Some(snapshot_data("Old title", 100)),
            },
        )
        .expect("the first Link should be created");
        let second = decide(
            first.state,
            Event::LinkExternalObject {
                item_id: 2,
                object,
                snapshot: None,
            },
        )
        .expect("the second Link should be created");
        let refreshed = decide(
            second.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: snapshot_data("New title", 200),
            },
        )
        .expect("the shared object should refresh once");
        assert_eq!(
            home_view(&refreshed.state, None, "now")
                .attention_entries
                .len(),
            2
        );

        let reviewed = decide(refreshed.state, Event::MarkLinkReviewed { link_id: 1 })
            .expect("mark reviewed should be explicit");
        let entries = home_view(&reviewed.state, None, "now").attention_entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].link_id, 2);
        assert_eq!(reviewed.state.links[0].reviewed_activity_id, 1);
        assert_eq!(reviewed.state.links[1].reviewed_activity_id, 0);
    }

    #[test]
    fn an_item_can_carry_multiple_reminders_and_due_reminders_need_attention() {
        let first = decide(
            state_with_item(7, "Work"),
            Event::AddItemReminder {
                item_id: 1,
                remind_at: "2026-09-18T09:00".into(),
            },
        )
        .expect("the first Reminder should be added");
        let second = decide(
            first.state,
            Event::AddItemReminder {
                item_id: 1,
                remind_at: "2026-09-25T09:00".into(),
            },
        )
        .expect("the second Reminder should be added");

        let view = home_view(&second.state, None, "2026-09-19T09:00");
        assert_eq!(second.state.items[0].reminders.len(), 2);
        assert_eq!(view.attention_entries.len(), 1);
        assert_eq!(view.attention_entries[0].kind, AttentionEntryKind::Reminder);
        assert_eq!(
            view.attention_entries[0].reminder_id,
            Some(second.state.items[0].reminders[0].id)
        );
        assert_eq!(view.needs_attention[0].item.id, 1);
        assert_eq!(second.state.items[0].status, ItemStatus::Inbox);

        let removed = decide(
            second.state,
            Event::RemoveItemReminder {
                item_id: 1,
                reminder_id: 1,
            },
        )
        .expect("removing one Reminder should leave the other one intact");
        assert_eq!(removed.state.items[0].reminders.len(), 1);
        assert_eq!(
            removed.state.items[0].reminders[0].remind_at,
            "2026-09-25T09:00"
        );
    }

    #[test]
    fn watch_until_and_review_at_are_independent_and_review_date_needs_attention() {
        let linked = decide(
            state_with_item(7, "Work"),
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::Generic,
                    kind: ExternalObjectKind::Generic,
                    external_key: "https://example.com/spec".into(),
                    canonical_url: "https://example.com/spec".into(),
                },
                snapshot: Some(snapshot_data("Spec", 100)),
            },
        )
        .expect("the External Object should be linkable without execution");
        let watched = decide(
            linked.state,
            Event::SetLinkWatchUntil {
                link_id: 1,
                watch_until: Some("2026-09-20T09:00".into()),
            },
        )
        .expect("watch_until should be configurable");
        let scheduled = decide(
            watched.state,
            Event::SetLinkReviewAt {
                link_id: 1,
                review_at: Some("2026-09-25T09:00".into()),
            },
        )
        .expect("review_at should be configurable independently");

        assert_eq!(
            scheduled.state.links[0].watch_until.as_deref(),
            Some("2026-09-20T09:00")
        );
        assert_eq!(
            scheduled.state.links[0].review_at.as_deref(),
            Some("2026-09-25T09:00")
        );

        let refreshed = decide(
            scheduled.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: snapshot_data("Changed spec", 200),
            },
        )
        .expect("the watched External Object should still record Activity");
        let after_watch = home_view(&refreshed.state, None, "2026-09-21T09:00");
        assert_eq!(refreshed.state.activities.len(), 1);
        assert_eq!(after_watch.attention_entries.len(), 0);

        let at_review = home_view(&refreshed.state, None, "2026-09-25T09:00");
        assert_eq!(at_review.attention_entries.len(), 1);
        assert_eq!(
            at_review.attention_entries[0].kind,
            AttentionEntryKind::Review
        );
        assert_eq!(at_review.attention_entries[0].link_id, 1);
        assert_eq!(refreshed.state.items[0].status, ItemStatus::Inbox);

        let watch_extended = decide(
            refreshed.state.clone(),
            Event::SetLinkWatchUntil {
                link_id: 1,
                watch_until: Some("2026-09-30T09:00".into()),
            },
        )
        .expect("the watch period should remain independent");
        let both_due = home_view(&watch_extended.state, None, "2026-09-25T09:00");
        assert_eq!(both_due.attention_entries.len(), 2);
        assert!(both_due
            .attention_entries
            .iter()
            .any(|entry| entry.kind == AttentionEntryKind::ExternalChange));
        assert!(both_due
            .attention_entries
            .iter()
            .any(|entry| entry.kind == AttentionEntryKind::Review));
        let changes_only = decide(
            watch_extended.state,
            Event::ClearLinkReviewAt { link_id: 1 },
        )
        .expect("clearing the review date should leave change attention alone");
        let changes_only_view = home_view(&changes_only.state, None, "2026-09-25T09:00");
        assert_eq!(changes_only_view.attention_entries.len(), 1);
        assert_eq!(
            changes_only_view.attention_entries[0].kind,
            AttentionEntryKind::ExternalChange
        );

        let cleared = decide(refreshed.state, Event::ClearLinkReviewAt { link_id: 1 })
            .expect("the reached review date should be dismissible explicitly");
        assert!(home_view(&cleared.state, None, "2026-09-25T00:00")
            .attention_entries
            .is_empty());
        assert_eq!(
            cleared.state.links[0].watch_until.as_deref(),
            Some("2026-09-20T09:00")
        );
        assert_eq!(cleared.state.items[0].status, ItemStatus::Inbox);
    }

    fn snapshot_data(title: &str, fetched_at: i64) -> ExternalSnapshotData {
        ExternalSnapshotData {
            title: title.into(),
            state: "OPEN".into(),
            metadata: Vec::new(),
            fetched_at,
        }
    }

    fn state_with_context(id: i64, name: &str) -> DomainState {
        state_with_contexts(&[(id, name)])
    }

    fn state_with_item(id: i64, name: &str) -> DomainState {
        let mut state = state_with_context(id, name);
        state.items.push(Item {
            id: 1,
            human_identifier: "MC-1".into(),
            title: "Existing Item".into(),
            project_id: 1,
            status: ItemStatus::Inbox,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.next_item_id = 2;
        state.next_item_number = 2;
        state
    }

    fn state_with_contexts(contexts: &[(i64, &str)]) -> DomainState {
        DomainState {
            next_context_id: contexts.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1,
            next_project_id: contexts.len() as i64 + 1,
            next_item_id: 1,
            next_item_number: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: contexts
                .iter()
                .map(|(id, name)| Context {
                    id: *id,
                    name: (*name).into(),
                })
                .collect(),
            projects: contexts
                .iter()
                .enumerate()
                .map(|(index, (context_id, _))| Project {
                    id: index as i64 + 1,
                    context_id: *context_id,
                    name: "Default".into(),
                    defaults: ProjectDefaults {
                        item_status: ItemStatus::Inbox,
                    },
                })
                .collect(),
            items: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }

    fn empty_state() -> DomainState {
        DomainState {
            next_context_id: 1,
            next_project_id: 1,
            next_item_id: 1,
            next_item_number: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: Vec::new(),
            projects: Vec::new(),
            items: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }
}
