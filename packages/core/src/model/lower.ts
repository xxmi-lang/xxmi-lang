/**
 * CST → model lowering (docs/01-language-model.md §2 "Parser contract"). Structure comes from the
 * DLL-faithful line scanner; meaning within lines (references, declarations, statement kinds)
 * comes from tree-sitter, run on each section separately. Never throws on file content.
 */
import {
  parseSpans,
  type SyntaxIssue,
  type SyntaxNode,
  type TextEncoding,
} from '../parser/index.ts';
import type { SpecLookup } from '../spec/lookup.ts';
import { scanIni, type ScannedLine, type TextSpan } from './scan.ts';
import type {
  Access,
  IniFile,
  LineModel,
  Reference,
  SectionModel,
  SymbolKind,
  VariableDeclaration,
} from './types.ts';

export interface LowerOptions {
  path: string;
  text: string;
  encoding: TextEncoding;
  /**
   * Namespace the DLL gives the file when it has no `namespace =`: its path relative to the
   * 3DMigoto folder with `\` separators, or '' for the main d3dx.ini.
   */
  defaultNamespace: string;
  lookup: SpecLookup;
}

/** Nodes whose text is a reference, and the kind they refer to. */
const REFERENCE_NODES: Record<string, SymbolKind> = {
  callable_commandlist: 'commandlist',
  callable_customshader: 'customshader',
  custom_resource: 'resource',
  resource_pool: 'pool',
  preset_section_identifier: 'preset',
  named_variable: 'variable',
};

/** Statements whose `name` field is written, not read. */
const ASSIGNMENTS = new Set([
  'assignment_statement',
  'key_assignment_statement',
  'preset_assignment_statement',
]);

const DECLARATIONS = new Set([
  'global_declaration',
  'global_initialisation',
  'local_declaration',
  'local_initialisation',
]);

export async function lowerIni(options: LowerOptions): Promise<IniFile> {
  const { text, lookup } = options;
  const scanned = scanIni(text);

  const file: IniFile = {
    path: options.path,
    text,
    encoding: options.encoding,
    namespace: options.defaultNamespace,
    preamble: scanned.preamble.map((l) => ({ ...l })),
    preambleIssues: [],
    sections: [],
    comments: scanned.comments,
    references: [],
    variables: [],
  };

  // ParseIniPreamble: `namespace = …` renames the file's namespace, `condition = …` gates it.
  for (const line of scanned.preamble) {
    const key = line.key?.text.toLowerCase();
    if (key === 'namespace' && line.value) {
      file.namespace = line.value.text;
      file.namespaceDeclaration = line.value;
    } else if (key === 'condition' && line.value) {
      file.condition = line.value;
    }
  }

  for (const s of scanned.sections) {
    const spec = lookup.sectionKind(s.name.text);
    const prefix = lookup.sectionPrefix(s.name.text);
    const qualifiedName =
      prefix && file.namespace !== ''
        ? `${prefix}\\${file.namespace}\\${s.name.text.slice(prefix.length)}`
        : s.name.text;
    const section: SectionModel = {
      name: s.name,
      header: s.header,
      chunk: s.chunk,
      qualifiedName,
      lines: s.lines.map((l) => lineModel(l, spec?.kind === 'commandlist')),
      syntaxIssues: [],
    };
    if (spec) {
      section.spec = spec;
      const kind = lookup.symbolKind(spec);
      if (kind) section.kind = kind;
    }
    file.sections.push(section);
  }

  const hasPreamble = /\S/.test(text.slice(scanned.preambleChunk.start, scanned.preambleChunk.end));
  const spans: TextSpan[] = [
    ...(hasPreamble ? [scanned.preambleChunk] : []),
    ...scanned.sections.map((s) => s.chunk),
  ];
  const results = await parseSpans(text, spans);
  results.forEach((result, i) => {
    const sectionIndex = hasPreamble ? i - 1 : i;
    try {
      if (sectionIndex < 0) {
        file.preambleIssues = result.issues;
        collect(result.tree.rootNode, file, -1, file.preamble);
      } else {
        const section = file.sections[sectionIndex];
        if (section) {
          section.syntaxIssues = result.issues;
          collect(result.tree.rootNode, file, sectionIndex, section.lines);
        }
      }
    } finally {
      result.tree.delete();
    }
  });
  return file;
}

function lineModel(line: ScannedLine, isCommandList: boolean): LineModel {
  const model: LineModel = { ...line };
  const key = line.key?.text ?? line.content.text;
  let command = key.toLowerCase();
  if (isCommandList) {
    // ParseCommandList: a `post ` or `pre ` prefix picks the list; the rest is the command.
    const m = /^(pre|post)\s+/.exec(command);
    if (m?.[1] === 'pre' || m?.[1] === 'post') {
      model.modifier = m[1];
      command = command.slice(m[0].length);
    }
  }
  if (line.key) model.command = command;
  return model;
}

/** Walks one section's tree for statements, references and variable declarations. */
function collect(root: SyntaxNode, file: IniFile, section: number, lines: LineModel[]): void {
  const byLine = new Map(lines.map((l) => [l.line, l]));
  const spanOf = (n: SyntaxNode): TextSpan => ({
    start: n.startIndex,
    end: n.endIndex,
    range: {
      start: { line: n.startPosition.row, character: n.startPosition.column },
      end: { line: n.endPosition.row, character: n.endPosition.column },
    },
  });

  const visit = (node: SyntaxNode): void => {
    if (node.isError) return; // text inside an ERROR is reported as XM001, not interpreted
    const line = byLine.get(node.startPosition.row);
    if (line && line.statement === undefined && isStatement(node.type)) line.statement = node.type;

    if (DECLARATIONS.has(node.type)) {
      const variable = node.childForFieldName('variable');
      if (variable) {
        const flags = node.children.map((c) => c.type);
        const declaration: VariableDeclaration = {
          name: variable.text.trim(),
          span: spanOf(variable),
          section,
          scope: node.type.startsWith('local') ? 'local' : 'global',
          persist: flags.includes('persist'),
          locked: flags.includes('locked'),
        };
        file.variables.push(declaration);
      }
      const value = node.childForFieldName('value') ?? node.childForFieldName('expression');
      if (value) visit(value);
      return;
    }

    const kind = REFERENCE_NODES[node.type];
    if (kind) {
      const text = referenceText(node);
      if (text) {
        const ref: Reference = { kind, text, span: spanOf(node), section, access: access(node) };
        file.references.push(ref);
      }
      // Pool index expressions and member arguments can hold more references.
      for (const child of node.namedChildren) {
        if (child.type === 'index_expression' || child.type === 'arguments') visit(child);
      }
      return;
    }

    for (const child of node.namedChildren) visit(child);
  };
  visit(root);
}

function isStatement(type: string): boolean {
  return (
    type.endsWith('_statement') ||
    type.endsWith('_instruction') ||
    type.endsWith('_declaration') ||
    type.endsWith('_initialisation')
  );
}

/** Reference text as written; a pool's index is not part of its name. */
function referenceText(node: SyntaxNode): string {
  const index = node.namedChildren.find((c) => c.type === 'index_expression');
  const end = index ? index.startIndex - node.startIndex : undefined;
  return node.text.slice(0, end).trim();
}

function access(node: SyntaxNode): Access {
  let target: SyntaxNode = node;
  // `$PoolFoo[0] = …` wraps the pool in a pooled_variable.
  if (target.parent?.type === 'pooled_variable') target = target.parent;
  const parent = target.parent;
  if (!parent) return 'read';
  if (ASSIGNMENTS.has(parent.type) && parent.childForFieldName('name')?.equals(target))
    return 'write';
  if (
    parent.type === 'store_instruction' &&
    parent.namedChildren.find((c) => c.type !== 'instruction')?.equals(target)
  ) {
    return 'write';
  }
  return 'read';
}

export type { SyntaxIssue };
