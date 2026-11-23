//! DLL-faithful lookups over the spec: section kinds by prefix or exact name, commands by exact
//! name then prefix.

use std::collections::{HashMap, HashSet};

use crate::model::SymbolKind;
use crate::spec::{
    CommandKind, MatchKind, SectionKind, Spec, SpecCommand, SpecEnum, SpecKey, SpecSection,
};

/// Lower-cased equality without allocating for the ASCII names nearly every key and value has.
pub fn eq_caseless(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a.to_lowercase() == b.to_lowercase()
}

fn address(section: &SpecSection) -> usize {
    std::ptr::from_ref(section) as usize
}

/// The first key of a name wins, as in a linear search.
fn key_table(section: &'static SpecSection) -> HashMap<String, &'static SpecKey> {
    let mut table = HashMap::new();
    for key in &section.keys {
        table.entry(key.name.to_lowercase()).or_insert(key);
    }
    table
}

pub struct Lookup {
    spec: &'static Spec,
    /// Command-list sections first, as `IsCommandListSection` is checked before `IsRegularSection`.
    sections: Vec<(String, &'static SpecSection)>,
    /// Keys by lower-cased name, per section (by address): sections have dozens of keys and every
    /// line of a file looks one up.
    keys: HashMap<usize, HashMap<String, &'static SpecKey>>,
    exact_commands: HashMap<String, &'static SpecCommand>,
    prefix_commands: Vec<(String, &'static SpecCommand)>,
    enums: HashMap<&'static str, &'static SpecEnum>,
    builtins: HashSet<String>,
}

impl Lookup {
    pub fn new(spec: &'static Spec) -> Self {
        let of_kind = |kind| spec.sections.iter().filter(move |s| s.kind == kind);
        let sections = of_kind(SectionKind::CommandList)
            .chain(of_kind(SectionKind::Regular))
            .map(|s| (s.name.to_lowercase(), s))
            .collect();

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
                MatchKind::Prefix => prefix_commands.push((command.name.to_lowercase(), command)),
            }
        }

        let keys = spec
            .sections
            .iter()
            .map(|section| (address(section), key_table(section)))
            .collect();

        Self {
            spec,
            sections,
            keys,
            exact_commands,
            prefix_commands,
            enums: spec.enums.iter().map(|e| (e.name.as_str(), e)).collect(),
            builtins: spec
                .builtin_sections
                .iter()
                .map(|b| b.name.to_lowercase())
                .collect(),
        }
    }

    pub fn spec(&self) -> &'static Spec {
        self.spec
    }

    /// The section kind a `[name]` belongs to: command-list table first, then the regular one.
    pub fn section_kind(&self, name: &str) -> Option<&'static SpecSection> {
        self.matching_section(name).map(|(_, section)| *section)
    }

    /// The namespacing prefix of `name` (`SectionPrefix`): only prefix entries count. Returns the
    /// prefix as spelled in the DLL table.
    pub fn section_prefix(&self, name: &str) -> Option<&'static str> {
        let lower = name.to_lowercase();
        self.sections
            .iter()
            .find(|(prefix, s)| s.prefix && lower.starts_with(prefix))
            .map(|(_, s)| s.name.as_str())
    }

    /// The symbol a section of this spec kind declares, if it declares one.
    pub fn symbol_kind(&self, section: &SpecSection) -> Option<SymbolKind> {
        SymbolKind::of_section_table(&section.name.to_lowercase())
    }

    /// The command a command-list key dispatches to: exact names first, then prefixes.
    pub fn command(&self, key: &str) -> Option<&'static SpecCommand> {
        let lower = key.to_lowercase();
        self.exact_commands.get(&lower).copied().or_else(|| {
            self.prefix_commands
                .iter()
                .find(|(prefix, _)| lower.starts_with(prefix))
                .map(|(_, command)| *command)
        })
    }

    pub fn key(&self, section: &'static SpecSection, key: &str) -> Option<&'static SpecKey> {
        let table = self.keys.get(&address(section))?;
        table.get(&key.to_lowercase()).copied()
    }

    pub fn enum_table(&self, name: &str) -> Option<&'static SpecEnum> {
        self.enums.get(name).copied()
    }

    pub fn is_builtin_section(&self, name: &str) -> bool {
        self.builtins.contains(&name.to_lowercase())
    }

    /// Commands written as a key (everything but the value-form ones).
    pub fn key_commands(&self) -> impl Iterator<Item = &'static SpecCommand> {
        self.spec.commands.iter().filter(|c| c.is_key_command())
    }

    /// Commands the DLL matches on the right-hand side (`run = BuiltInCommandListUnbindAllRenderTargets`).
    pub fn value_commands(&self) -> impl Iterator<Item = &'static SpecCommand> {
        self.spec.commands.iter().filter(|c| !c.is_key_command())
    }

    /// Names of every command that is written as a key (value-form commands are not keys).
    pub fn key_command_names(&self) -> impl Iterator<Item = &'static str> {
        self.spec
            .commands
            .iter()
            .filter(|c| c.is_key_command())
            .map(|c| c.name.as_str())
    }

    fn matching_section(&self, name: &str) -> Option<&(String, &'static SpecSection)> {
        let lower = name.to_lowercase();
        self.sections.iter().find(|(section_name, s)| {
            if s.prefix {
                lower.starts_with(section_name)
            } else {
                lower == *section_name
            }
        })
    }
}
