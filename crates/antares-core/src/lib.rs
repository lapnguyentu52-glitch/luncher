//! antares-core — typed state container, event hub, task registry (§88.1)

pub mod app_state;
pub mod error;
pub mod events;
pub mod tasks;

pub use app_state::AppState;
pub use error::{CoreError, CoreResult};
pub use events::{EventEnvelope, EventHub, EventQos, EventTopic};
pub use tasks::{Task, TaskPriority, TaskRegistry, TaskState};
