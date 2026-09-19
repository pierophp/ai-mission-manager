use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub context_id: i64,
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
    pub next_item_id: i64,
    pub next_item_number: i64,
    pub contexts: Vec<Context>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    CreateItem { title: String, context_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
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
    #[error("an Item title cannot be blank")]
    EmptyTitle,
    #[error("Context {context_id} does not exist")]
    ContextNotFound { context_id: i64 },
    #[error("the Item identifier sequence is exhausted")]
    SequenceExhausted,
}

pub fn decide(mut state: DomainState, event: Event) -> Result<Decision, DomainError> {
    match event {
        Event::CreateItem { title, context_id } => {
            if title.trim().is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            if !state
                .contexts
                .iter()
                .any(|context| context.id == context_id)
            {
                return Err(DomainError::ContextNotFound { context_id });
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
                context_id,
                status: ItemStatus::Inbox,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_an_item_puts_it_in_inbox_and_requests_persistence() {
        let state = state_with_context(7, "Work");

        let decision = decide(
            state,
            Event::CreateItem {
                title: "Investigate timeout".into(),
                context_id: 7,
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
                context_id: 7,
                status: ItemStatus::Inbox,
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
    fn item_identifiers_use_one_global_sequence_across_contexts() {
        let state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);

        let first = decide(
            state,
            Event::CreateItem {
                title: "Work item".into(),
                context_id: 7,
            },
        )
        .expect("first item should succeed");
        let second = decide(
            first.state,
            Event::CreateItem {
                title: "Personal item".into(),
                context_id: 8,
            },
        )
        .expect("second item should succeed");

        assert_eq!(second.state.items[0].human_identifier, "MC-1");
        assert_eq!(second.state.items[1].human_identifier, "MC-2");
    }

    #[test]
    fn item_creation_rejects_blank_titles_and_unknown_contexts() {
        let state = state_with_context(7, "Work");

        assert_eq!(
            decide(
                state.clone(),
                Event::CreateItem {
                    title: "   ".into(),
                    context_id: 7,
                },
            ),
            Err(DomainError::EmptyTitle)
        );
        assert_eq!(
            decide(
                state,
                Event::CreateItem {
                    title: "No context".into(),
                    context_id: 99,
                },
            ),
            Err(DomainError::ContextNotFound { context_id: 99 })
        );
    }

    fn state_with_context(id: i64, name: &str) -> DomainState {
        state_with_contexts(&[(id, name)])
    }

    fn state_with_contexts(contexts: &[(i64, &str)]) -> DomainState {
        DomainState {
            next_item_id: 1,
            next_item_number: 1,
            contexts: contexts
                .iter()
                .map(|(id, name)| Context {
                    id: *id,
                    name: (*name).into(),
                })
                .collect(),
            items: Vec::new(),
        }
    }
}
