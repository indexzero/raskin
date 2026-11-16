//! Capture persisted macOS preferences and generate inspectable native commands.
//!
//! Generation is pure: it never reads a host or runs the emitted commands.
//! See `docs/schema.md` and `docs/coverage.md` for representation and coverage.
pub mod artifact;
pub mod capture;
pub mod codec;
mod error;
pub mod model;
pub mod presets;
pub mod render;
mod support;

pub use error::{Error, Result};
pub use model::{Document, Domain, Host, Properties, Reason, Scope, Setting, Unavailable, Value};
