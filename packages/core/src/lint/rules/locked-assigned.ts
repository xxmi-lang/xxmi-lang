import type { IniFile, Reference } from '../../model/types.ts';
import type { Rule } from '../types.ts';

const PRESET_SECTIONS = new Set(['Key', 'Preset']);

/** Command-list assignments, found by the parser. */
function commandListWrites(file: IniFile): Reference[] {
  return file.references.filter(
    (ref) =>
      ref.kind === 'variable' &&
      ref.access === 'write' &&
      file.sections[ref.section]?.spec?.kind === 'commandlist',
  );
}

/**
 * The `$var = …` keys of [Key] and [Preset] sections: they aren't command lists,
 * so the parser records no reference for them.
 */
function presetWrites(file: IniFile): Reference[] {
  return file.sections.flatMap((section, index) => {
    if (!PRESET_SECTIONS.has(section.spec?.name ?? '')) return [];
    return section.lines.flatMap((line) =>
      line.key?.text.startsWith('$')
        ? [
            {
              kind: 'variable' as const,
              text: line.key.text,
              span: line.key.span,
              section: index,
              access: 'write' as const,
            },
          ]
        : [],
    );
  });
}

/**
 * ParseCommandListVariableAssignment (CommandList.cpp) reads the target through GetVariable,
 * which fails with "Unable to assign value to <locked> variable" in any namespace,
 * and the line is dropped. Override::ParseIniSection (Override.cpp) reads the `$var` keys of
 * [Key] and [Preset] sections through the same GetVariable and skips the line the same way.
 */
export const XM212: Rule = {
  id: 'XM212',
  severity: 'error',
  description: 'Assignment to a locked global (the DLL rejects the line)',
  check({ file, symbols, report }) {
    for (const ref of [...commandListWrites(file), ...presetWrites(file)]) {
      if (!symbols.resolveVariable(ref, file)?.locked) continue;

      report(
        ref.span.range,
        `Global ${ref.text} is \`locked\`; assigning it is an error and the DLL drops the line.`,
      );
    }
  },
};
