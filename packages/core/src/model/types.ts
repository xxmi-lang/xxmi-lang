/**
 * The typed model of one ini file (docs/01-language-model.md §3). Plain data: no tree-sitter
 * nodes survive lowering, so a model can be cached, serialized and compared.
 */
import type { SyntaxIssue, TextEncoding } from '../parser/index.ts';
import type { SpecSection } from '../spec/types.ts';
import type { ScannedLine, ScannedText, TextSpan } from './scan.ts';

export type { ScannedLine, ScannedText, TextSpan } from './scan.ts';

/** What a reference or a declared section can point at. */
export type SymbolKind =
  | 'commandlist'
  | 'customshader'
  | 'resource'
  | 'pool'
  | 'preset'
  | 'key'
  | 'textureoverride'
  | 'shaderoverride'
  | 'shaderregex'
  | 'variable';

export interface LineModel extends ScannedLine {
  /** `pre ` / `post ` prefix on a command-list key. */
  modifier?: 'pre' | 'post';
  /** Lower-cased key with any `pre `/`post ` prefix removed (what the DLL dispatches on). */
  command?: string;
  /** Type of the tree-sitter statement node that starts on this line, if any. */
  statement?: string;
}

export interface SectionModel {
  /** The name as written between `[` and `]`. */
  name: ScannedText;
  header: ScannedLine;
  chunk: TextSpan;
  /** The section kind, or undefined for an unknown prefix (XM101). */
  spec?: SpecSection;
  /** Symbol kind this section declares (a `[CommandList…]` declares a `commandlist`). */
  kind?: SymbolKind;
  /**
   * Name the DLL stores the section under: `Prefix\namespace\Local` for a prefixed section in a
   * namespaced file, else the name as written. Compare case-insensitively.
   */
  qualifiedName: string;
  lines: LineModel[];
  /** ERROR/MISSING nodes from parsing this section on its own. */
  syntaxIssues: SyntaxIssue[];
}

export type Access = 'read' | 'write';

export interface Reference {
  kind: SymbolKind;
  /** As written, e.g. `CommandList\ZZMI\SetTextures`, `ResourceFoo`, `$\TTL\alpha`. */
  text: string;
  span: TextSpan;
  /** Index into `IniFile.sections`, or -1 for the preamble. */
  section: number;
  access: Access;
}

export interface VariableDeclaration {
  /** As written, e.g. `$alpha`. */
  name: string;
  span: TextSpan;
  section: number;
  scope: 'global' | 'local';
  persist: boolean;
  locked: boolean;
}

export interface IniFile {
  /** Host path of the file. */
  path: string;
  text: string;
  encoding: TextEncoding;
  /** Effective namespace: `namespace =` if declared, else the path-derived default ('' for d3dx.ini). */
  namespace: string;
  namespaceDeclaration?: ScannedText;
  /** Preamble `condition = …`, which makes the DLL skip the file when false. */
  condition?: ScannedText;
  preamble: LineModel[];
  preambleIssues: SyntaxIssue[];
  sections: SectionModel[];
  /** `;` comment lines, for doc comments and suppression directives. */
  comments: ScannedLine[];
  references: Reference[];
  variables: VariableDeclaration[];
}
