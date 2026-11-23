import type { IniFile, VariableDeclaration } from '../../model/types.ts';
import { globalVariableKey } from '../../workspace/symbols.ts';
import type { Workspace } from '../../workspace/workspace.ts';
import { loadedFiles, loadsBefore } from './loaded-files.ts';
import { relativeLocation } from './structure.ts';

/** A declaration: its file and its index into `IniFile.variables`. */
export interface Site {
  file: IniFile;
  index: number;
}

export interface Kept {
  site: Site;
  locked: boolean;
}

/** ParseConstantsSection only registers globals from [Constants]. */
export function constantsGlobals(
  file: IniFile,
): { variable: VariableDeclaration; index: number }[] {
  return file.variables
    .map((variable, index) => ({ variable, index }))
    .filter(
      ({ variable }) =>
        variable.scope === 'global' && file.sections[variable.section]?.spec?.name === 'Constants',
    );
}

/**
 * RegisterGlobalVariable refuses a name it already holds,
 * so the first [Constants] declaration in load order wins, flags included.
 */
export function keptGlobals(workspace: Workspace, file: IniFile): Map<string, Kept> {
  const kept = new Map<string, Kept>();
  for (const other of loadedFiles(workspace, file)) {
    for (const { variable, index } of constantsGlobals(other)) {
      const key = globalVariableKey(variable.name, other.namespace);
      const first = kept.get(key);
      if (
        !first ||
        (first.site.file !== other && loadsBefore(workspace, file, other, first.site.file))
      ) {
        kept.set(key, { site: { file: other, index }, locked: variable.locked });
      }
    }
  }
  return kept;
}

/** Where a declaration is, as seen from `from`. */
export function describeSite(from: IniFile, site: Site): string {
  const line = (site.file.variables[site.index]?.span.range.start.line ?? 0) + 1;
  return site.file === from ? `line ${line}` : `${relativeLocation(from, site.file)}:${line}`;
}
