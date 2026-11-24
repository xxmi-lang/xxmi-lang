//! Game profiles: the model importer (MI) a mod targets decides which library it is checked
//! against (docs/01-language-model.md "5. Game profiles").

mod detect;
mod registry;

pub use detect::{Evidence, detect};
pub use registry::{Dialect, PROFILES, Profile, by_id, generic, ids};
