//! CST to model lowering (port of packages/core/src/model/lower.ts). Structure comes from the
//! DLL-faithful line scanner; meaning within lines (references, declarations, statement kinds)
//! comes from tree-sitter, run on each section separately. Never fails on file content.

use std::path::PathBuf;

use crate::document::{Chunk, Document};
use crate::encoding::Encoding;
use crate::lookup::Lookup;
use crate::parse::collect_syntax_issues;
use crate::scan::{ScannedIni, ScannedLine, Span};
use crate::spec::SectionKind;

use super::collect::collect;
use super::types::{IniFile, Issue, LineModel, SectionModel};

pub struct LowerOptions<'a> {
    pub path: PathBuf,
    pub text: String,
    pub encoding: Encoding,
    /// Namespace the DLL gives the file when it has no `namespace =`: its path relative to the
    /// 3DMigoto folder with `\` separators, or '' for the main d3dx.ini.
    pub default_namespace: String,
    pub lookup: &'a Lookup,
}

pub fn lower_ini(options: LowerOptions) -> IniFile {
    let LowerOptions {
        path,
        text,
        encoding,
        default_namespace,
        lookup,
    } = options;
    let doc = Document::new(text);
    let scan = doc.scan();
    let text = doc.text();

    let mut preamble = line_models(text, &scan.preamble, false);
    let namespace = declared_namespace(text, &preamble).unwrap_or(default_namespace);
    let mut sections = section_models(text, scan, &namespace, lookup);

    let mut references = Vec::new();
    let mut variables = Vec::new();
    let mut preamble_issues = Vec::new();
    if let Some(chunk) = doc.preamble_chunk() {
        let mut target = ChunkTarget::new(None, 0, &mut preamble, &mut preamble_issues);
        analyse(text, chunk, &mut target, &mut references, &mut variables);
    }
    for (index, section) in sections.iter_mut().enumerate() {
        let line = section.header.line;
        let mut target =
            ChunkTarget::new(Some(index), line, &mut section.lines, &mut section.issues);
        analyse(
            text,
            doc.section_chunk(index),
            &mut target,
            &mut references,
            &mut variables,
        );
    }

    IniFile {
        path,
        text: text.to_string(),
        encoding,
        namespace,
        preamble,
        preamble_issues,
        sections,
        comments: scan.comments.clone(),
        references,
        variables,
        line_index: doc.line_index().clone(),
    }
}

/// What lowering fills in for one chunk: the lines it annotates and the issues it collects.
pub(super) struct ChunkTarget<'a> {
    pub section: Option<usize>,
    /// File line the chunk starts on.
    pub line: u32,
    pub lines: &'a mut [LineModel],
    pub issues: &'a mut Vec<Issue>,
}

impl<'a> ChunkTarget<'a> {
    fn new(
        section: Option<usize>,
        line: u32,
        lines: &'a mut [LineModel],
        issues: &'a mut Vec<Issue>,
    ) -> Self {
        Self {
            section,
            line,
            lines,
            issues,
        }
    }
}

/// ParseIniPreamble: `namespace = ...` renames the file's namespace.
fn declared_namespace(text: &str, preamble: &[LineModel]) -> Option<String> {
    preamble.iter().find_map(|line| {
        let key = line.scanned.key?.slice(text).to_lowercase();
        let value = line.scanned.value?;
        (key == "namespace").then(|| value.slice(text).to_string())
    })
}

fn section_models(
    text: &str,
    scan: &ScannedIni,
    namespace: &str,
    lookup: &Lookup,
) -> Vec<SectionModel> {
    scan.sections
        .iter()
        .map(|scanned| {
            let name = scanned.name.slice(text);
            let spec = lookup.section_kind(name);
            let is_command_list = spec.is_some_and(|s| s.kind == SectionKind::CommandList);

            SectionModel {
                name: scanned.name,
                header: scanned.header.clone(),
                chunk: scanned.chunk,
                spec,
                kind: spec.and_then(|s| lookup.symbol_kind(s)),
                qualified_name: qualified_name(name, namespace, lookup),
                lines: line_models(text, &scanned.lines, is_command_list),
                issues: Vec::new(),
            }
        })
        .collect()
}

/// `Prefix\namespace\Local` for a prefixed section in a namespaced file, else the name as written.
fn qualified_name(name: &str, namespace: &str, lookup: &Lookup) -> String {
    match lookup.section_prefix(name) {
        Some(prefix) if !namespace.is_empty() => {
            let local = name.get(prefix.len()..).unwrap_or_default();
            format!("{prefix}\\{namespace}\\{local}")
        }
        _ => name.to_string(),
    }
}

fn line_models(text: &str, lines: &[ScannedLine], is_command_list: bool) -> Vec<LineModel> {
    lines
        .iter()
        .map(|line| LineModel {
            scanned: line.clone(),
            command: line
                .key
                .map(|key| command_of(key.slice(text), is_command_list)),
            statement: None,
        })
        .collect()
}

/// What the DLL dispatches on: the lower-cased key, minus the `pre `/`post ` list selector of a
/// command-list line (ParseCommandList).
fn command_of(key: &str, is_command_list: bool) -> String {
    let lower = key.to_lowercase();
    if !is_command_list {
        return lower;
    }

    for modifier in ["pre", "post"] {
        if let Some(rest) = lower.strip_prefix(modifier)
            && rest.starts_with(char::is_whitespace)
        {
            return rest.trim_start().to_string();
        }
    }
    lower
}

/// Reads one chunk's tree: its syntax issues (shifted to file offsets), then its statements,
/// references and declarations.
fn analyse(
    text: &str,
    chunk: &Chunk,
    target: &mut ChunkTarget,
    references: &mut Vec<super::types::Reference>,
    variables: &mut Vec<super::types::VariableDeclaration>,
) {
    let source = chunk.span.slice(text);
    for issue in collect_syntax_issues(&chunk.tree, source) {
        target.issues.push(Issue {
            span: shift(issue.span, chunk.span),
            line: target.line + issue.row as u32,
            message: issue.message,
        });
    }
    collect(chunk, source, target, references, variables);
}

fn shift(span: Span, chunk: Span) -> Span {
    Span {
        start: span.start + chunk.start,
        end: span.end + chunk.start,
    }
}
