//! The typed model of an ini file and the lowering that builds it.

mod collect;
mod lower;
pub mod mesh;
mod types;

pub use collect::ChunkAnalysis;
pub use lower::{LowerOptions, lower_ini};
pub use types::{
    Access, IniFile, Issue, LineModel, Reference, Scope, SectionModel, SymbolKind,
    VariableDeclaration,
};
