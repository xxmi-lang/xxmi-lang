//! Spec-driven lint core for XXMI / 3DMigoto mod `.ini` files: the Rust port of `@xxmi-lang/core`.

pub mod defaults;
pub mod diagnostic;
pub mod document;
pub mod encoding;
pub mod glob;
pub mod lint;
pub mod lookup;
pub mod model;
pub mod parse;
pub mod paths;
pub mod rules;
pub mod scan;
pub mod spec;
pub mod suggest;
pub mod workspace;
