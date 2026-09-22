//! Feature-oriented seams for the application layer.
//!
//! Feature-oriented application seams.
//!
//! Feature modules own the Runtime operations and Tauri adapters for the
//! product capabilities. `app` remains a deliberately small stable command
//! facade, while the largest feature is split into internal Work modules.

pub(crate) mod activity;
pub(crate) mod deletion;
pub(crate) mod setup;
pub(crate) mod structure;
pub(crate) mod work;

/// The one application state holder shared by every feature seam.
pub(crate) use crate::app::Runtime;

/// The concrete state stored in Tauri remains one `Mutex<Runtime>`.
pub(crate) type SharedRuntime = std::sync::Mutex<Runtime>;
