//! "Where does the DLL implement this?" (`find_implementation`):
//! spec entries carry the DLL source symbol they were extracted from
//! (`CommandList.cpp:ParseCommandListGeneralCommands`);
//! this resolves that symbol to lines of a DLL source tree, plus other mentions of the keyword.
//! Port of packages/core/src/query/implementation.ts.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder, escape};
use serde::Serialize;
use xxmi_core::lookup::Lookup;
use xxmi_core::query::spec::SpecEntry;
use xxmi_core::query::spec_all::{SpecQuery, lookup_spec};

use crate::cancel::cancelled;

use super::slice_utf16;
use super::tree::SourceTree;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HitRole {
    Definition,
    Table,
    Literal,
    Mention,
}

impl HitRole {
    pub fn as_str(self) -> &'static str {
        match self {
            HitRole::Definition => "definition",
            HitRole::Table => "table",
            HitRole::Literal => "literal",
            HitRole::Mention => "mention",
        }
    }

    fn rank(self) -> u8 {
        match self {
            HitRole::Definition | HitRole::Table => 0,
            HitRole::Literal => 1,
            HitRole::Mention => 2,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ImplementationHit {
    pub path: String,
    /// 1-based.
    pub line: usize,
    pub role: HitRole,
    pub snippet: String,
    /// The function the line is in: a guess from the nearest unindented signature above it.
    #[serde(rename = "inFunction", skip_serializing_if = "Option::is_none")]
    pub in_function: Option<String>,
    /// The spec entry this hit was found for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Added by the tool once the checkout's commit is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
}

pub struct ImplementationResult {
    pub entries: Vec<SpecEntry>,
    pub hits: Vec<ImplementationHit>,
}

const MAX_FILE_BYTES: usize = 2_000_000;
const MENTION_CAP: usize = 10;
const SNIPPET_CHARS: usize = 200;
const SYMBOL_HIT_CAP: usize = 3;
const SIGNATURE_LOOKBACK: usize = 600;

/// `\w` and `\b` as JavaScript reads them: ASCII only.
static SIGNATURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][(?-u:\w):<>*&,\s]*?(?-u:\b)([A-Za-z_](?-u:\w)*(?:::~?(?-u:\w)+)*)\s*\(")
        .expect("signature pattern")
});
static NOT_A_SIGNATURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:if|for|while|switch|return|else|do|case|using|typedef)(?-u:\b)")
        .expect("keyword pattern")
});
static SOURCE_SPLIT: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^(.+?\.(?:cpp|h|hpp|c)):(.+)$")
        .case_insensitive(true)
        .build()
        .expect("source pattern")
});
static SOURCE_FILE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\.(?:cpp|h|hpp|c)$")
        .case_insensitive(true)
        .build()
        .expect("source file pattern")
});

fn sensitive(pattern: &str) -> Regex {
    Regex::new(pattern).expect("escaped pattern")
}

fn insensitive(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .expect("escaped pattern")
}

fn split_source(source: &str) -> Option<(&str, &str)> {
    let found = SOURCE_SPLIT.captures(source)?;
    Some((found.get(1)?.as_str(), found.get(2)?.as_str()))
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn resolve_file(tree: &dyn SourceTree, file: &str) -> Option<String> {
    let lower = file.to_lowercase();
    let suffix = format!("/{lower}");
    tree.files()
        .iter()
        .filter(|f| {
            let candidate = f.to_lowercase();
            candidate == lower || candidate.ends_with(&suffix)
        })
        .min_by_key(|f| utf16_len(f))
        .cloned()
}

fn enclosing_function(lines: &[&str], index: usize) -> Option<String> {
    let floor = index.saturating_sub(SIGNATURE_LOOKBACK);
    for i in (floor..index).rev() {
        let text = lines[i];
        if NOT_A_SIGNATURE.is_match(text) || text.trim_end().ends_with(';') {
            continue;
        }
        if let Some(name) = SIGNATURE.captures(text).and_then(|c| c.get(1)) {
            return Some(name.as_str().to_string());
        }
    }
    None
}

fn snippet_of(line: &str) -> String {
    slice_utf16(line.trim(), SNIPPET_CHARS)
}

fn hit(
    path: &str,
    index: usize,
    role: HitRole,
    text: &str,
    entry: Option<&str>,
) -> ImplementationHit {
    ImplementationHit {
        path: path.to_string(),
        line: index + 1,
        role,
        snippet: snippet_of(text),
        in_function: None,
        entry: entry.map(String::from),
        permalink: None,
    }
}

fn symbol_hits(path: &str, lines: &[&str], symbol: &str, entry: &str) -> Vec<ImplementationHit> {
    if symbol == "literal" {
        return Vec::new();
    }

    let name = escape(symbol);
    let definition = sensitive(&format!(r"^\S.*(?-u:\b){name}\s*\("));
    let table = sensitive(&format!(r"^\S.*(?-u:\b){name}(?-u:\b)\s*(?:\[|=|\{{|$)"));
    let mut hits = Vec::new();
    for (i, text) in lines.iter().enumerate() {
        let role = if definition.is_match(text) && !text.trim_end().ends_with(';') {
            Some(HitRole::Definition)
        } else if table.is_match(text) && !text.contains('(') {
            Some(HitRole::Table)
        } else {
            None
        };
        if let Some(role) = role {
            hits.push(hit(path, i, role, text, Some(entry)));
        }
    }
    hits.truncate(SYMBOL_HIT_CAP);
    hits
}

fn literal_hits(
    path: &str,
    lines: &[&str],
    name: &str,
    role: HitRole,
    entry: &str,
) -> Vec<ImplementationHit> {
    let pattern = insensitive(&format!("L?\"{}\"", escape(name)));
    let mut hits = Vec::new();
    for (i, text) in lines.iter().enumerate() {
        if !pattern.is_match(text) {
            continue;
        }
        let mut found = hit(path, i, role, text, Some(entry));
        found.in_function = enclosing_function(lines, i);
        hits.push(found);
    }
    hits
}

fn read_text(tree: &dyn SourceTree, path: &str) -> Option<String> {
    let text = tree.read(path)?;
    (utf16_len(&text) <= MAX_FILE_BYTES).then_some(text)
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

fn entry_hits(entry: &SpecEntry, tree: &dyn SourceTree) -> Vec<ImplementationHit> {
    let Some((file, symbol)) = split_source(&entry.source) else {
        return Vec::new();
    };
    let Some(path) = resolve_file(tree, file) else {
        return Vec::new();
    };
    let Some(text) = read_text(tree, &path) else {
        return Vec::new();
    };

    let lines = split_lines(&text);
    let label = format!("{} {}", entry.kind, entry.name);
    let name = entry.name.strip_prefix("->").unwrap_or(&entry.name);
    let mut hits = symbol_hits(&path, &lines, symbol, &label);
    hits.extend(literal_hits(&path, &lines, name, HitRole::Literal, &label));
    hits
}

/// Stable, so hits of one role keep their order.
fn rank_hits(mut hits: Vec<ImplementationHit>) -> Vec<ImplementationHit> {
    hits.sort_by_key(|h| h.role.rank());
    hits
}

/// The best DLL source location for a spec entry: its symbol's definition, else a literal.
pub fn locate_spec_entry(entry: &SpecEntry, tree: &dyn SourceTree) -> Option<ImplementationHit> {
    rank_hits(entry_hits(entry, tree)).into_iter().next()
}

/// Ends early once the call has timed out; the tool reports that after the scan.
fn source_files(tree: &dyn SourceTree) -> impl Iterator<Item = &String> {
    tree.files()
        .iter()
        .take_while(|_| !cancelled())
        .filter(|f| SOURCE_FILE.is_match(f))
}

fn mention_hits(
    tree: &dyn SourceTree,
    name: &str,
    skip: &HashSet<String>,
) -> Vec<ImplementationHit> {
    let mut hits = Vec::new();
    for path in source_files(tree) {
        let Some(text) = read_text(tree, path) else {
            continue;
        };
        let lines = split_lines(&text);
        for found in literal_hits(path, &lines, name, HitRole::Mention, name) {
            if !skip.contains(&format!("{}:{}", found.path, found.line)) {
                hits.push(found);
            }
            if hits.len() >= MENTION_CAP {
                return hits;
            }
        }
    }
    hits
}

/// Word-boundary text matches, for features that are not spec entries.
fn word_hits(tree: &dyn SourceTree, word: &str) -> Vec<ImplementationHit> {
    let pattern = insensitive(&format!(r"(?-u:\b){}(?-u:\b)", escape(word)));
    let mut hits = Vec::new();
    for path in source_files(tree) {
        let Some(text) = read_text(tree, path) else {
            continue;
        };
        for (i, line) in split_lines(&text).iter().enumerate() {
            if hits.len() < MENTION_CAP * 2 && pattern.is_match(line) {
                hits.push(hit(path, i, HitRole::Mention, line, None));
            }
        }
    }
    hits
}

/// Ranked DLL source locations for an ini feature (`store`, `Pool`, `->Region`, `handling`).
pub fn find_implementation(
    lookup: &'static Lookup,
    feature: &str,
    tree: &dyn SourceTree,
    limit: usize,
) -> ImplementationResult {
    let entries = lookup_spec(
        lookup,
        &SpecQuery {
            name: feature,
            ..SpecQuery::default()
        },
    );
    let mut hits: Vec<ImplementationHit> =
        entries.iter().flat_map(|e| entry_hits(e, tree)).collect();

    let bare = feature.trim();
    let bare = bare.strip_prefix("->").unwrap_or(bare);
    if entries.is_empty() {
        hits.extend(word_hits(tree, bare));
    } else if utf16_len(bare) >= 3 {
        let seen: HashSet<String> = hits
            .iter()
            .map(|h| format!("{}:{}", h.path, h.line))
            .collect();
        hits.extend(mention_hits(tree, bare, &seen));
    }

    let mut ranked = rank_hits(hits);
    ranked.truncate(limit);
    ImplementationResult {
        entries,
        hits: ranked,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct MemoryTree {
        files: Vec<String>,
        texts: HashMap<String, String>,
    }

    impl MemoryTree {
        fn new(files: &[(&str, &str)]) -> Self {
            Self {
                files: files.iter().map(|(p, _)| (*p).to_string()).collect(),
                texts: files
                    .iter()
                    .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
                    .collect(),
            }
        }
    }

    impl SourceTree for MemoryTree {
        fn files(&self) -> &[String] {
            &self.files
        }

        fn read(&self, path: &str) -> Option<String> {
            self.texts.get(path).cloned()
        }
    }

    #[test]
    fn splits_a_source_symbol() {
        assert_eq!(
            split_source("CommandList.cpp:ParseGeneral"),
            Some(("CommandList.cpp", "ParseGeneral"))
        );
        assert_eq!(split_source("manual"), None);
    }

    #[test]
    fn finds_enclosing_function_and_skips_statements() {
        let lines = [
            "void Foo::Bar(int x)",
            "{",
            "    if (x) {",
            "        L\"store\";",
        ];
        assert_eq!(enclosing_function(&lines, 3).as_deref(), Some("Foo::Bar"));
    }

    #[test]
    fn ranks_definitions_before_literals_and_mentions() {
        let tree = MemoryTree::new(&[(
            "DirectX11/CommandList.cpp",
            "static void Parse(const wchar_t *key)\n{\n    if (!wcscmp(key, L\"Store\"))\n        return;\n}\n",
        )]);
        let entry_lines = split_lines(&tree.texts["DirectX11/CommandList.cpp"]);
        let hits = symbol_hits(
            "DirectX11/CommandList.cpp",
            &entry_lines,
            "Parse",
            "command store",
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].role, HitRole::Definition);

        let literals = literal_hits("p", &entry_lines, "store", HitRole::Literal, "e");
        assert_eq!(literals[0].line, 3);
        assert_eq!(literals[0].in_function.as_deref(), Some("Parse"));

        let ranked = rank_hits(vec![
            literals.into_iter().next().unwrap(),
            hits.into_iter().next().unwrap(),
        ]);
        assert_eq!(ranked[0].role, HitRole::Definition);
    }
}
