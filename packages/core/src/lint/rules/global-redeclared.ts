import type { IniFile } from '../../model/types.ts';
import { globalVariableKey } from '../../workspace/symbols.ts';
import type { Rule } from '../types.ts';
import { constantsGlobals } from './kept-globals.ts';
import { lockedRedeclaredIndices } from './locked-redeclared.ts';

/** A declaration the DLL drops, and the earlier one it keeps; both index `IniFile.variables`. */
export interface Redeclaration {
  index: number;
  first: number;
}

/**
 * RegisterGlobalVariable refuses a name it already holds,
 * so the first declaration wins and the DLL warns "Redeclaration".
 */
export function redeclaredGlobals(file: IniFile): Redeclaration[] {
  const firstByKey = new Map<string, number>();
  const redeclared: Redeclaration[] = [];

  for (const { variable, index } of constantsGlobals(file)) {
    const key = globalVariableKey(variable.name, file.namespace);
    const first = firstByKey.get(key);
    if (first === undefined) firstByKey.set(key, index);
    else redeclared.push({ index, first });
  }
  return redeclared;
}

export const XM209: Rule = {
  id: 'XM209',
  severity: 'warning',
  description: 'Global declared twice in [Constants] (the DLL keeps the first)',
  check({ file, workspace, report }) {
    const redeclared = redeclaredGlobals(file);
    // XM211 owns the redeclarations of a locked global.
    const locked =
      redeclared.length > 0 ? lockedRedeclaredIndices(workspace, file) : new Set<number>();

    for (const { index, first } of redeclared) {
      if (locked.has(index)) continue;

      const variable = file.variables[index];
      const firstDeclaration = file.variables[first];
      if (!variable || !firstDeclaration) continue;

      const firstLine = firstDeclaration.span.range.start.line + 1;
      report(
        variable.span.range,
        `Global ${variable.name} is already declared on line ${firstLine}; the DLL keeps the first and ignores this one.`,
      );
    }
  },
};
