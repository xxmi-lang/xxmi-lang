//! DLL-faithful lookups over the spec: section kinds by prefix or exact name, commands by exact
//! name then prefix (port of `SpecLookup` in packages/core/src/spec/lookup.ts).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::spec::{
    CommandKind, MatchKind, SectionKind, Spec, SpecCommand, SpecKey, SpecSection, spec,
};

pub struct Lookup {
    spec: &'static Spec,
    exact_commands: HashMap<String, &'static SpecCommand>,
    prefix_commands: Vec<&'static SpecCommand>,
}

pub fn lookup() -> &'static Lookup {
    static LOOKUP: OnceLock<Lookup> = OnceLock::new();
    LOOKUP.get_or_init(|| Lookup::new(spec()))
}

impl Lookup {
    fn new(spec: &'static Spec) -> Self {
        let mut exact_commands = HashMap::new();
        let mut prefix_commands = Vec::new();
        let dispatched = spec.commands.iter().filter(|c| {
            !matches!(c.kind, CommandKind::Flow | CommandKind::Prefix) && c.is_key_command()
        });
        for command in dispatched {
            match command.match_kind {
                MatchKind::Exact => {
                    exact_commands.insert(command.name.to_lowercase(), command);
                }
                MatchKind::Prefix => prefix_commands.push(command),
            }
        }
        Self {
            spec,
            exact_commands,
            prefix_commands,
        }
    }

    /// The section kind a `[name]` belongs to: command-list table first, then the regular one.
    pub fn section_kind(&self, name: &str) -> Option<&'static SpecSection> {
        let lower = name.to_lowercase();
        let of_kind = |kind| self.spec.sections.iter().filter(move |s| s.kind == kind);
        of_kind(SectionKind::CommandList)
            .chain(of_kind(SectionKind::Regular))
            .find(|s| section_matches(s, &lower))
    }

    /// The command a command-list key dispatches to: exact names first, then prefixes.
    pub fn command(&self, key: &str) -> Option<&'static SpecCommand> {
        let lower = key.to_lowercase();
        self.exact_commands.get(&lower).copied().or_else(|| {
            self.prefix_commands
                .iter()
                .copied()
                .find(|c| lower.starts_with(&c.name.to_lowercase()))
        })
    }

    pub fn key(&self, section: &'static SpecSection, key: &str) -> Option<&'static SpecKey> {
        let lower = key.to_lowercase();
        section.keys.iter().find(|k| k.name.to_lowercase() == lower)
    }

    /// Names of every command that is written as a key (value-form commands are not keys).
    pub fn key_command_names(&self) -> impl Iterator<Item = &'static str> {
        self.spec
            .commands
            .iter()
            .filter(|c| c.is_key_command())
            .map(|c| c.name.as_str())
    }
}

fn section_matches(section: &SpecSection, lower_name: &str) -> bool {
    let section_name = section.name.to_lowercase();
    if section.prefix {
        lower_name.starts_with(&section_name)
    } else {
        lower_name == section_name
    }
}
