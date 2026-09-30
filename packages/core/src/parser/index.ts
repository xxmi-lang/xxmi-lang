/**
 * Parser wrapper around our fork of tree-sitter-migoto, loaded as WASM through web-tree-sitter.
 * See docs/01-language-model.md §2.
 */
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { Language, Parser, type Node, type Tree } from 'web-tree-sitter';

export type { Node as SyntaxNode, Tree as SyntaxTree } from 'web-tree-sitter';

/** Zero-based line and UTF-16 column, the same units LSP uses by default. */
export interface Position {
  line: number;
  character: number;
}

export interface Range {
  start: Position;
  end: Position;
}

/**
 * A node the grammar could not fit into the tree. `error` is a tree-sitter ERROR node
 * (unexpected text), `missing` is a MISSING node the parser inserted to recover.
 */
export interface SyntaxIssue {
  kind: 'error' | 'missing';
  range: Range;
  /** UTF-16 offsets into the parsed text. */
  startOffset: number;
  endOffset: number;
  message: string;
}

export type TextEncoding = 'utf8' | 'utf8bom' | 'unknown';

export interface ParseResult {
  /** Owns WASM memory: call `tree.delete()` when done with it. */
  tree: Tree;
  issues: SyntaxIssue[];
}

export interface FileParseResult extends ParseResult {
  path: string;
  /** Decoded text the tree was built from (without a BOM). */
  text: string;
  /** `unknown`: the bytes aren't valid UTF-8 (likely GBK or Shift-JIS) and were decoded lossily. */
  encoding: TextEncoding;
}

export interface InitParserOptions {
  /** Grammar WASM as a path or bytes. Defaults to the build shipped in `grammar/`. */
  grammarWasm?: string | Uint8Array;
}

const BUNDLED_GRAMMAR = fileURLToPath(
  new URL('../../grammar/tree-sitter-migoto.wasm', import.meta.url),
);

let language: Promise<Language> | undefined;
let sharedParser: Promise<Parser> | undefined;

/**
 * Loads the tree-sitter runtime and the grammar. Idempotent: later calls return the first
 * load's language and ignore their options.
 */
export function initParser(options: InitParserOptions = {}): Promise<Language> {
  language ??= (async () => {
    await Parser.init();
    return Language.load(options.grammarWasm ?? BUNDLED_GRAMMAR);
  })().catch((error: unknown) => {
    language = undefined;
    throw error;
  });
  return language;
}

/** A new parser for callers that keep their own state, such as incremental reparsing in the LSP. */
export async function createParser(): Promise<Parser> {
  const lang = await initParser();
  return new Parser().setLanguage(lang);
}

/** Parses `text` and collects ERROR and MISSING nodes. */
export async function parseText(text: string): Promise<ParseResult> {
  sharedParser ??= createParser();
  const tree = (await sharedParser).parse(text);
  if (!tree) {
    throw new Error('tree-sitter returned no tree (grammar not loaded?)');
  }
  return { tree, issues: collectSyntaxIssues(tree) };
}

/** A slice of the text to parse on its own, in UTF-16 offsets and LSP positions. */
export interface ParseSpan {
  start: number;
  end: number;
  range: Range;
}

/**
 * Parses each span of `text` as if it were the whole document, keeping absolute positions
 * (tree-sitter `includedRanges`). Used to parse sections independently, so a syntax error can't
 * spill into the next section. Each returned tree must be deleted by the caller.
 */
export async function parseSpans(text: string, spans: ParseSpan[]): Promise<ParseResult[]> {
  sharedParser ??= createParser();
  const parser = await sharedParser;
  return spans.map((span) => {
    const tree = parser.parse(text, null, {
      includedRanges: [
        {
          startIndex: span.start,
          endIndex: span.end,
          startPosition: { row: span.range.start.line, column: span.range.start.character },
          endPosition: { row: span.range.end.line, column: span.range.end.character },
        },
      ],
    });
    if (!tree) throw new Error('tree-sitter returned no tree (grammar not loaded?)');
    return { tree, issues: collectSyntaxIssues(tree) };
  });
}

/** Reads, decodes and parses one file. */
export async function parseFile(path: string): Promise<FileParseResult> {
  const { text, encoding } = decodeText(await readFile(path));
  return { path, text, encoding, ...(await parseText(text)) };
}

/**
 * Decodes file bytes. A UTF-8 BOM is stripped so offsets match what editors show. Invalid UTF-8
 * is decoded lossily and reported as `unknown`, so parsing and later diagnostics still run.
 */
export function decodeText(bytes: Uint8Array): { text: string; encoding: TextEncoding } {
  const hasBom = bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
  const body = hasBom ? bytes.subarray(3) : bytes;
  try {
    const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(body);
    return { text, encoding: hasBom ? 'utf8bom' : 'utf8' };
  } catch {
    return {
      text: new TextDecoder('utf-8', { ignoreBOM: true }).decode(body),
      encoding: 'unknown',
    };
  }
}

/**
 * Returns the outermost ERROR nodes and all MISSING nodes, in document order. Nodes nested inside
 * an ERROR belong to that error and aren't reported separately.
 */
export function collectSyntaxIssues(tree: Tree): SyntaxIssue[] {
  const issues: SyntaxIssue[] = [];
  const visit = (node: Node): void => {
    if (node.isMissing) {
      issues.push(toIssue(node, 'missing', `Missing ${describeMissing(node)}`));
      return;
    }
    if (node.isError) {
      issues.push(toIssue(node, 'error', `Syntax error: unexpected ${quoteSnippet(node.text)}`));
      return;
    }
    for (const child of node.children) {
      if (child.hasError || child.isMissing) visit(child);
    }
  };
  if (tree.rootNode.hasError) visit(tree.rootNode);
  return issues;
}

function toIssue(node: Node, kind: SyntaxIssue['kind'], message: string): SyntaxIssue {
  return {
    kind,
    range: {
      start: { line: node.startPosition.row, character: node.startPosition.column },
      end: { line: node.endPosition.row, character: node.endPosition.column },
    },
    startOffset: node.startIndex,
    endOffset: node.endIndex,
    message,
  };
}

function describeMissing(node: Node): string {
  return node.isNamed ? node.type : `"${node.type}"`;
}

const SNIPPET_LENGTH = 60;

/** First non-blank line of `text`, shortened, in backticks. */
function quoteSnippet(text: string): string {
  const line = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .find((l) => l.length > 0);
  if (line === undefined) return 'end of line';
  const short = line.length > SNIPPET_LENGTH ? `${line.slice(0, SNIPPET_LENGTH)}…` : line;
  return `\`${short}\``;
}
