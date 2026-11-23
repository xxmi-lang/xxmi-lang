import type { IniFile, SectionModel, VariableDeclaration } from '../../model/types.ts';
import type { Rule } from '../types.ts';
import { flowKeyword } from './structure.ts';
import type { Redeclaration } from './global-redeclared.ts';

/**
 * declare_local_variable (CommandList.cpp) refuses a name that find_local_variable already sees
 * in the section's scope stack, so sibling branches may reuse a name but a nested one may not
 * shadow it. `if` opens a scope; `elif` and `else` clear the deepest one; `endif` closes it.
 */
export function redeclaredLocals(file: IniFile): Redeclaration[] {
  const redeclared: Redeclaration[] = [];
  file.sections.forEach((section, sectionIndex) => {
    if (section.spec?.kind !== 'commandlist') return;

    const locals = file.variables
      .map((variable, index) => ({ variable, index }))
      .filter(({ variable }) => variable.scope === 'local' && variable.section === sectionIndex);
    redeclared.push(...sectionRedeclarations(file, section, locals));
  });
  return redeclared;
}

function sectionRedeclarations(
  file: IniFile,
  section: SectionModel,
  locals: { variable: VariableDeclaration; index: number }[],
): Redeclaration[] {
  const scopes: Map<string, number>[] = [new Map<string, number>()];
  const redeclared: Redeclaration[] = [];
  let next = 0;

  for (const line of section.lines) {
    const keyword = flowKeyword(line);
    if (keyword === 'if') scopes.push(new Map());
    else if ((keyword === 'elif' || keyword === 'else') && scopes.length > 1) {
      scopes.pop();
      scopes.push(new Map());
    } else if (keyword === 'endif' && scopes.length > 1) scopes.pop();

    let local = locals[next];
    while (local && local.variable.span.start < line.content.span.end) {
      next++;

      const lower = local.variable.name.toLowerCase();
      const first = scopes.map((scope) => scope.get(lower)).find((found) => found !== undefined);
      if (first !== undefined) redeclared.push({ index: local.index, first });
      else scopes.at(-1)?.set(lower, local.index);

      local = locals[next];
    }
  }
  return redeclared;
}

export const XM210: Rule = {
  id: 'XM210',
  severity: 'error',
  description:
    'Local redeclared where an earlier local of the same name is visible (the DLL drops the line)',
  check({ file, report }) {
    for (const { index, first } of redeclaredLocals(file)) {
      const variable = file.variables[index];
      const firstDeclaration = file.variables[first];
      if (!variable || !firstDeclaration) continue;

      const firstLine = firstDeclaration.span.range.start.line + 1;
      report(
        variable.span.range,
        `Local ${variable.name} is already declared on line ${firstLine} in this or an enclosing branch; the DLL rejects this redeclaration.`,
      );
    }
  },
};
