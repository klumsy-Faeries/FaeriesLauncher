//! Foundation crate for the Faeries Launcher.
//!
//! Everything here is launcher-generic: no Minecraft, modding, or UI knowledge.
//! Higher crates (`faerie-minecraft`, `faerie-modding`, …) build on these
//! primitives; the Tauri app wires them together.

pub mod config;
pub mod error;
pub mod events;
pub mod logging;
pub mod paths;
pub mod secret;
pub mod tasks;

pub use error::CoreError;
pub use events::{Event, EventBus, TaskOutcome};
pub use paths::DataPaths;
pub use secret::Secret;
pub use tasks::{TaskContext, TaskHandle, TaskScheduler};
