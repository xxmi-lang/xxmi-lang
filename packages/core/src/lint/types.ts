import type { Range } from '../parser/index.ts';
import type { IniFile } from '../model/types.ts';
import type { PathResolver } from '../paths.ts';
import type { SpecLookup } from '../spec/lookup.ts';
import type { SymbolTable } from '../workspace/symbols.ts';
import type { Workspace } from '../workspace/workspace.ts';

export type Severity = 'error' | 'warning' | 'info' | 'hint';

/** A quick fix: edits to the diagnostic's own file. */
export interface Fix {
  title: string;
  edits: { range: Range; newText: string }[];
}

export interface Diagnostic {
  /** Rule id, e.g. `XM201`. */
  id: string;
  severity: Severity;
  message: string;
  /** Host path of the file. */
  path: string;
  range: Range;
  fix?: Fix;
}

export interface RuleContext {
  file: IniFile;
  workspace: Workspace;
  symbols: SymbolTable;
  lookup: SpecLookup;
  resolver: PathResolver;
  /** The 3DMigoto folder the file was loaded under, if found. */
  packageRoot: string | undefined;
  report: (range: Range, message: string, fix?: Fix) => void;
}

export interface Rule {
  id: string;
  severity: Severity;
  /** One line, shown by `xxmi query` and in docs/02-diagnostics.md. */
  description: string;
  check(context: RuleContext): void;
}
