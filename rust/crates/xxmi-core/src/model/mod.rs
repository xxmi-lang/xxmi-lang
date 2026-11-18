//! The typed model of an ini file and the lowering that builds it (packages/core/src/model).

mod collect;
mod lower;
mod types;

pub use lower::{LowerOptions, lower_ini};
pub use types::{
    Access, IniFile, Issue, LineModel, Reference, Scope, SectionModel, SymbolKind,
    VariableDeclaration,
};
