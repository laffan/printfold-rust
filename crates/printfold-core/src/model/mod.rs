//! Serde mirror of the TypeScript data model (`src/types/index.ts`).
//!
//! Field names serialise in camelCase and optional fields are omitted when
//! absent, so JSON produced here is indistinguishable from what the
//! frontend produces itself.

pub mod document;
pub mod fill;
pub mod items;
pub mod options;
pub mod project;

pub use document::*;
pub use fill::*;
pub use items::*;
pub use options::*;
pub use project::*;
