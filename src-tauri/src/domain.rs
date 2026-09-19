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
pub struct Item {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub project_id: i64,
    pub status: ItemStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Inbox,
    Active,
    Waiting,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainState {
    pub next_context_id: i64,
    pub next_project_id: i64,
    pub next_item_id: i64,
    pub next_item_number: i64,
    pub contexts: Vec<Context>,
    pub projects: Vec<Project>,
    pub items: Vec<Item>,
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
    #[error("the Item identifier sequence is exhausted")]
    SequenceExhausted,
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
    }
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

    fn state_with_context(id: i64, name: &str) -> DomainState {
        state_with_contexts(&[(id, name)])
    }

    fn state_with_contexts(contexts: &[(i64, &str)]) -> DomainState {
        DomainState {
            next_context_id: contexts.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1,
            next_project_id: contexts.len() as i64 + 1,
            next_item_id: 1,
            next_item_number: 1,
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
        }
    }

    fn empty_state() -> DomainState {
        DomainState {
            next_context_id: 1,
            next_project_id: 1,
            next_item_id: 1,
            next_item_number: 1,
            contexts: Vec::new(),
            projects: Vec::new(),
            items: Vec::new(),
        }
    }
}
