//! Editor navigation and assistance over the workspace model:
//! what is under the cursor, definition, references, rename, outline, hover and completion.
//! Frontends (LSP, MCP) only translate these results to their protocols.

mod completion;
mod hover;
mod markdown;
mod outline;
mod rename;
mod target;
mod text;

pub use completion::{CompletionItem, CompletionKind, completions};
pub use hover::{Hover, hover};
pub use outline::{OutlineSymbol, outline};
pub use rename::{RenameOutcome, rename};
pub use target::{Location, Symbol, Target, definition, references, target_at};
