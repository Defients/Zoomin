#[cfg(windows)]
mod capture;
pub mod geometry;
pub mod input;
pub mod inspector;
#[cfg(windows)]
pub mod measurement;
#[cfg(windows)]
mod native;
pub mod settings;
#[cfg(windows)]
pub use native::{Action, Engine, Snapshot};
