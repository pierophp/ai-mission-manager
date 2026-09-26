//! Pure domain model and state-transition facade.
//!
//! The domain is split by responsibility so feature code can depend on a small,
//! stable public vocabulary while the reducer remains the single conceptual
//! state-transition seam. No module in this directory performs I/O.

mod deletion;
mod error;
mod events;
mod grilling;
mod implementation_queue;
mod model;
mod projections;
mod reducer;

#[cfg(test)]
mod tests;

pub use deletion::*;
pub use error::*;
pub use events::*;
pub use grilling::*;
pub use implementation_queue::*;
pub use model::*;
pub use projections::*;
pub use reducer::*;

/// The shared pure state-transition seam used by all feature implementations.
///
/// Feature modules depend on this interface instead of importing projections,
/// adapters, or Tauri application code. The compatibility exports at
/// crate::domain remain available for existing callers.
pub mod state_transition {
    pub use super::{decide, Decision, DomainError, DomainState, Effect, Event};
}
