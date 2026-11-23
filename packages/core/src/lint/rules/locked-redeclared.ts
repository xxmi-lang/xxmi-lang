import type { IniFile } from '../../model/types.ts';
import { globalVariableKey } from '../../workspace/symbols.ts';
import type { Workspace } from '../../workspace/workspace.ts';
import type { Rule } from '../types.ts';
import { constantsGlobals, describeSite, keptGlobals, type Site } from './kept-globals.ts';

/** A later declaration of a locked global (an index into `IniFile.variables`) and the one kept. */
interface LockedRedeclaration {
  index: number;
  kept: Site;
}

/**
 * ParseConstantsSection (IniHandler.cpp): the first declaration wins, locked flag included,
 * so a redeclaration's value and modifiers never apply.
 */
function lockedRedeclarations(workspace: Workspace, file: IniFile): LockedRedeclaration[] {
  const declared = constantsGlobals(file);
  if (declared.length === 0) return [];

  const kept = keptGlobals(workspace, file);
  const result: LockedRedeclaration[] = [];
  for (const { variable, index } of declared) {
    const first = kept.get(globalVariableKey(variable.name, file.namespace));
    if (first?.locked && !(first.site.file === file && first.site.index === index)) {
      result.push({ index, kept: first.site });
    }
  }
  return result;
}

/** The declarations this rule reports, for XM209 to leave alone. */
export function lockedRedeclaredIndices(workspace: Workspace, file: IniFile): Set<number> {
  return new Set(lockedRedeclarations(workspace, file).map((r) => r.index));
}

export const XM211: Rule = {
  id: 'XM211',
  severity: 'error',
  description: 'Locked global declared again (the DLL keeps the first and ignores the new value)',
  check({ file, workspace, report }) {
    for (const { index, kept } of lockedRedeclarations(workspace, file)) {
      const variable = file.variables[index];
      if (!variable) continue;

      report(
        variable.span.range,
        `Global ${variable.name} is declared \`locked\` at ${describeSite(file, kept)}; the DLL keeps that declaration and ignores this one, so its value never applies.`,
      );
    }
  },
};
