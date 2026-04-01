pub mod application;
pub mod interfaces;

pub use application::state::{AppState, SharedState};
pub use interfaces::http::build_router;
