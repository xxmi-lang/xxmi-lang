//! The typed model of one ini file (docs/01-language-model.md §3).
//! Plain data: no tree-sitter nodes survive lowering.

use std::path::PathBuf;

use line_index::{LineIndex, TextSize, WideEncoding, WideLineCol};
use serde::{Deserialize, Serialize};

use crate::diagnostic::{Position, Range};
use crate::encoding::Encoding;
use crate::scan::{ScannedLine, Span};
use crate::spec::SpecSection;

/// What a reference or a declared section can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    CommandList,
    CustomShader,
    Resource,
    Pool,
    Preset,
    Key,
    TextureOverride,
    ShaderOverride,
    ShaderRegex,
    Variable,
}

impl SymbolKind {
    /// The symbol a section declares, by the lower-cased name of its DLL table entry.
    pub fn of_section_table(name: &str) -> Option<Self> {
        Some(match name {
            "commandlist" | "builtincommandlist" => Self::CommandList,
            "customshader" | "builtincustomshader" => Self::CustomShader,
            "resource" => Self::Resource,
            "pool" => Self::Pool,
            "preset" => Self::Preset,
            "key" => Self::Key,
            "textureoverride" => Self::TextureOverride,
            "shaderoverride" => Self::ShaderOverride,
            "shaderregex" => Self::ShaderRegex,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommandList => "commandlist",
            Self::CustomShader => "customshader",
            Self::Resource => "resource",
            Self::Pool => "pool",
            Self::Preset => "preset",
            Self::Key => "key",
            Self::TextureOverride => "textureoverride",
            Self::ShaderOverride => "shaderoverride",
            Self::ShaderRegex => "shaderregex",
            Self::Variable => "variable",
        }
    }
}

#[derive(Debug)]
pub struct LineModel {
    pub scanned: ScannedLine,
    /// Lower-cased key with any `pre `/`post ` prefix removed (what the DLL dispatches on).
    pub command: Option<String>,
    /// Kind of the first statement node tree-sitter starts on this line.
    pub statement: Option<String>,
}

/// A syntax issue with offsets into the whole file.
#[derive(Debug, Clone)]
pub struct Issue {
    pub span: Span,
    /// Zero-based line the issue starts on.
    pub line: u32,
    pub message: String,
}

#[derive(Debug)]
pub struct SectionModel {
    /// The name as written between `[` and `]`.
    pub name: Span,
    pub header: ScannedLine,
    /// From the start of the header's line to the next header.
    pub chunk: Span,
    /// `None` for an unknown prefix (XM101).
    pub spec: Option<&'static SpecSection>,
    /// Symbol kind this section declares.
    pub kind: Option<SymbolKind>,
    /// Name the DLL stores the section under: `Prefix\namespace\Local` for a prefixed section in a
    /// namespaced file, else the name as written. Compare case-insensitively.
    pub qualified_name: String,
    pub lines: Vec<LineModel>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

#[derive(Debug)]
pub struct Reference {
    pub kind: SymbolKind,
    /// As written, e.g. `CommandList\ZZMI\SetTextures`, `ResourceFoo`, `$\TTL\alpha`.
    pub text: String,
    pub span: Span,
    /// Index into `IniFile::sections`, or `None` for the preamble.
    pub section: Option<usize>,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Local,
}

#[derive(Debug)]
pub struct VariableDeclaration {
    /// As written, e.g. `$alpha`.
    pub name: String,
    pub span: Span,
    pub section: Option<usize>,
    pub scope: Scope,
    pub persist: bool,
    pub locked: bool,
}

pub struct IniFile {
    pub path: PathBuf,
    pub text: String,
    pub encoding: Encoding,
    /// Effective namespace: `namespace =` if declared, else the path-derived default ('' for d3dx.ini).
    pub namespace: String,
    pub preamble: Vec<LineModel>,
    pub preamble_issues: Vec<Issue>,
    pub sections: Vec<SectionModel>,
    /// `;` comment lines, for suppression directives.
    pub comments: Vec<ScannedLine>,
    pub references: Vec<Reference>,
    pub variables: Vec<VariableDeclaration>,
    pub line_index: LineIndex,
}

impl IniFile {
    pub fn slice(&self, span: Span) -> &str {
        span.slice(&self.text)
    }

    /// Byte span to LSP range (zero-based line, UTF-16 column).
    pub fn range_of(&self, span: Span) -> Range {
        Range {
            start: self.position_of(span.start),
            end: self.position_of(span.end),
        }
    }

    /// Byte offset of an LSP position, or `None` when the position isn't inside the text.
    pub fn offset_at(&self, pos: Position) -> Option<usize> {
        let wide = WideLineCol {
            line: pos.line,
            col: pos.character,
        };
        let line_col = self.line_index.to_utf8(WideEncoding::Utf16, wide)?;
        let offset = usize::from(self.line_index.offset(line_col)?);

        let valid = offset <= self.text.len() && self.text.is_char_boundary(offset);
        (valid && self.position_of(offset) == pos).then_some(offset)
    }

    fn position_of(&self, offset: usize) -> Position {
        let line_col = self.line_index.line_col(TextSize::new(offset as u32));
        // Offsets sit on char boundaries; the byte column is the fallback if one ever doesn't.
        let wide = self
            .line_index
            .to_wide(WideEncoding::Utf16, line_col)
            .unwrap_or(WideLineCol {
                line: line_col.line,
                col: line_col.col,
            });
        Position {
            line: wide.line,
            character: wide.col,
        }
    }
}
