/**
 * Library symbol snapshots (docs/01-language-model.md §4 "Bundled library snapshot"): the
 * sections, globals and namespaces of a package's libraries, so a mod linted on its own still
 * resolves `CommandList\ZZMI\…`.
 */
import { relative, sep } from 'node:path';
import { globalVariableKey, type SymbolSnapshot } from './symbols.ts';
import type { Workspace } from './workspace.ts';

export interface SnapshotSource {
  name: string;
  repository: string;
  commit: string;
}

/**
 * Symbols of every file the workspace loaded under `root`. Output is sorted, so the same package
 * commit gives the same bytes.
 */
export function buildSnapshot(
  workspace: Workspace,
  root: string,
  source: SnapshotSource,
): SymbolSnapshot {
  const rel = (path: string): string => relative(root, path).split(sep).join('/');
  const files = [...workspace.files.values()].filter(
    (f) => !relative(root, f.path).startsWith('..'),
  );

  const sections: SymbolSnapshot['sections'] = [];
  const variables: SymbolSnapshot['variables'] = [];
  const namespaces = new Set<string>();
  for (const file of files) {
    if (file.namespace !== '') namespaces.add(file.namespace);
    for (const section of file.sections) {
      if (!section.kind) continue;
      sections.push({
        kind: section.kind,
        qualifiedName: section.qualifiedName,
        path: rel(file.path),
        line: section.header.line + 1,
      });
    }
    for (const v of file.variables) {
      if (v.scope !== 'global') continue;
      variables.push({
        key: globalVariableKey(v.name, file.namespace),
        persist: v.persist,
        locked: v.locked,
        path: rel(file.path),
        line: v.span.range.start.line + 1,
      });
    }
  }
  const byText = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);
  return {
    version: 1,
    name: source.name,
    source: { repository: source.repository, commit: source.commit },
    namespaces: [...namespaces].sort(byText),
    sections: sections.sort(
      (a, b) =>
        byText(a.qualifiedName.toLowerCase(), b.qualifiedName.toLowerCase()) ||
        byText(a.path, b.path),
    ),
    variables: variables.sort((a, b) => byText(a.key, b.key) || byText(a.path, b.path)),
  };
}
