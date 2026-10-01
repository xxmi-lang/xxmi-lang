/**
 * Structure rules (XM1xx). Messages quote the DLL's own warning text where it has one
 * (IniHandler.cpp), so editor output matches the game log.
 */
import type { LineModel, SectionModel } from '../../model/types.ts';
import type { SpecKey } from '../../spec/types.ts';
import { didYouMean, editDistance, replaceFix } from '../suggest.ts';
import type { Rule, RuleContext } from '../types.ts';

/** `x`, `y1`, `w123`: ini params (the grammar's `ini_parameter`). */
const INI_PARAM = /^[xyzw]\d{0,3}$/i;

/** Sections whose content the DLL doesn't read as keys (e.g. `[Profile]`, parsed by nvprofile.cpp). */
function isFreeForm(section: SectionModel): boolean {
  return (
    section.spec !== undefined &&
    section.spec.allowsBareLines &&
    section.spec.keys.length === 0 &&
    section.spec.kind === 'regular'
  );
}

/** Flow-control keyword a command-list line starts with (`ParseCommandListFlowControl`). */
export function flowKeyword(line: LineModel): 'if' | 'elif' | 'else' | 'endif' | undefined {
  const text = line.content.text.toLowerCase();
  if (/^if\s/.test(text)) return 'if';
  if (/^(elif|else if)\s/.test(text)) return 'elif';
  if (text === 'else') return 'else';
  if (text === 'endif') return 'endif';
  return undefined;
}

export const XM101: Rule = {
  id: 'XM101',
  severity: 'error',
  description: 'Unknown section prefix (not in the command-list or regular section tables)',
  check({ file, lookup, report }) {
    const names = lookup.spec.sections.map((s) => s.name);
    for (const section of file.sections) {
      if (section.spec) continue;
      const name = section.name.text;
      // Compare each known prefix with the name's leading characters, allowing a couple of
      // missing or extra letters (`[TextureOverid…]`).
      const [suggestion] = names
        .map((n) => {
          const lengths = [-2, -1, 0, 1, 2].map((delta) => n.length + delta).filter((l) => l > 0);
          return { n, d: Math.min(...lengths.map((l) => editDistance(name.slice(0, l), n))) };
        })
        .filter(({ n, d }) => d > 0 && d <= Math.min(2, Math.floor(n.length / 3)))
        .sort((a, b) => a.d - b.d || a.n.localeCompare(b.n));
      const hint = suggestion ? ` Did you mean a [${suggestion.n}…] section?` : '';
      report(section.name.span.range, `Unknown section type [${name}].${hint}`);
    }
  },
};

export const XM102: Rule = {
  id: 'XM102',
  severity: 'warning',
  description: 'Unknown key for this section kind',
  check({ file, lookup, report }) {
    for (const section of file.sections) {
      const spec = section.spec;
      if (!spec || isFreeForm(section)) continue;
      const known = spec.keys.map((k) => k.name);
      for (const line of section.lines) {
        if (!line.key) continue;
        const key = line.key.text;
        if (spec.kind === 'commandlist') {
          // The grammar parses anything it doesn't recognise as a command, resource copy or
          // assignment as a generic `key = value` setting.
          if (line.statement !== 'setting_statement') continue;
          const command = line.command ?? key.toLowerCase();
          if (lookup.key(spec, command) || lookup.command(command)) continue;
          report(
            line.content.span.range,
            `Unrecognised entry: ${line.content.text}.${didYouMean(command, [...known, ...lookup.spec.commands.map((c) => c.name)])}`,
          );
          continue;
        }
        if (lookup.key(spec, key)) continue;
        const dynamic = spec.dynamicKeys.map((d) => d.kind);
        if (dynamic.includes('variable') && key.startsWith('$')) continue;
        if (dynamic.includes('ini_param') && INI_PARAM.test(key)) continue;
        const removed = spec.removedKeys.some((k) => k.name.toLowerCase() === key.toLowerCase());
        if (removed) {
          report(
            line.key.span.range,
            `\`${key}\` is read by vanilla 3DMigoto but not by XXMI; the DLL ignores it.`,
          );
          continue;
        }
        const none =
          known.length === 0 ? ' XXMI reads no keys from this section.' : didYouMean(key, known);
        report(
          line.key.span.range,
          `Unknown key \`${key}\` in [${section.name.text}]; the DLL ignores it.${none}`,
          replaceFix(line.key.span.range, key, known),
        );
      }
    }
  },
};

export const XM103: Rule = {
  id: 'XM103',
  severity: 'error',
  description: "Bare line (no `=`) in a section that doesn't allow it",
  check({ file, report }) {
    for (const section of file.sections) {
      if (!section.spec || section.spec.allowsBareLines) continue;
      for (const line of section.lines) {
        if (!line.key) report(line.content.span.range, `Malformed line: "${line.content.text}"`);
      }
    }
  },
};

export const XM104: Rule = {
  id: 'XM104',
  severity: 'warning',
  description: "Duplicate key not in the DLL's duplicate whitelist",
  check({ file, lookup, report }) {
    for (const section of file.sections) {
      const spec = section.spec;
      if (!spec) continue;
      // Global sections in included files may override the main d3dx.ini's values.
      const prefixed = lookup.sectionPrefix(section.name.text) !== undefined;
      if (file.namespace !== '' && !prefixed) continue;
      const seen = new Map<string, LineModel>();
      for (const line of section.lines) {
        if (!line.key) continue;
        const name =
          (spec.kind === 'commandlist' ? line.command : line.key.text.toLowerCase()) ?? '';
        let key: SpecKey | undefined;
        if (spec.kind === 'commandlist') {
          // Only keys parsed outside the command list are checked ("non-command list key").
          key = lookup.key(spec, name);
          if (!key) continue;
        } else {
          if (spec.allowsDuplicateKeys) continue;
          key = lookup.key(spec, name);
        }
        if (key?.repeatable) continue;
        const first = seen.get(name);
        if (first) {
          const what =
            spec.kind === 'commandlist'
              ? 'Duplicate non-command list key found'
              : 'Duplicate key found';
          report(
            line.key.span.range,
            `${what}: ${line.key.text} (first on line ${first.line + 1}; the DLL uses the first)`,
          );
        } else {
          seen.set(name, line);
        }
      }
    }
  },
};

export const XM105: Rule = {
  id: 'XM105',
  severity: 'error',
  description: 'Invalid enum value (resource `type`, `format`, key `type`, …)',
  check({ file, lookup, report }) {
    for (const section of file.sections) {
      const spec = section.spec;
      if (!spec) continue;
      for (const line of section.lines) {
        if (!line.key || !line.value?.text) continue;
        const key = lookup.key(
          spec,
          spec.kind === 'commandlist' ? (line.command ?? '') : line.key.text,
        );
        const table = /^enum:(\w+)$/.exec(key?.valueType ?? '')?.[1];
        if (!table) continue;
        const values = lookup.enumTable(table)?.values.map((v) => v.name);
        if (!values || values.length === 0) continue;
        let value = line.value.text;
        if (table === 'DXGIFormats') {
          // ParseFormatString: numeric value, or a name with an optional DXGI_FORMAT_ prefix.
          if (/^\d+$/.test(value)) continue;
          value = value.replace(/^DXGI_FORMAT_/i, '');
        }
        if (values.some((v) => v.toLowerCase() === value.toLowerCase())) continue;
        const prefix = line.value.text.slice(0, line.value.text.length - value.length);
        report(
          line.value.span.range,
          `Invalid ${line.key.text} "${line.value.text}".${didYouMean(value, values)}`,
          replaceFix(line.value.span.range, value, values, prefix),
        );
      }
    }
  },
};

export const XM106: Rule = {
  id: 'XM106',
  severity: 'error',
  description: 'Unbalanced `if`/`elif`/`else`/`endif`',
  check({ file, report }) {
    for (const section of file.sections) {
      if (section.spec?.kind !== 'commandlist') continue;
      const open: { line: LineModel; sawElse: boolean }[] = [];
      for (const line of section.lines) {
        const keyword = flowKeyword(line);
        if (!keyword) continue;
        const top = open.at(-1);
        if (keyword === 'if') open.push({ line, sawElse: false });
        else if (keyword === 'endif') {
          if (top) open.pop();
          else report(line.content.span.range, '`endif` without a matching `if`');
        } else if (!top) {
          report(
            line.content.span.range,
            `\`${line.content.text.split(/\s/)[0] ?? ''}\` without a matching \`if\``,
          );
        } else if (top.sawElse) {
          report(line.content.span.range, `\`${keyword}\` after \`else\` in the same \`if\` block`);
        } else if (keyword === 'else') {
          top.sawElse = true;
        }
      }
      for (const { line } of open) {
        report(
          line.content.span.range,
          `Scope unbalanced: \`if\` without \`endif\` in [${section.name.text}]`,
        );
      }
    }
  },
};

export const XM107: Rule = {
  id: 'XM107',
  severity: 'warning',
  description: 'Duplicate section name within the same namespace',
  check({ file, workspace, lookup, report }) {
    // The DLL keeps the first and warns for the rest, in load order (d3dx.ini, then includes).
    const firstByName = new Map<string, { path: string; line: number }>();
    for (const other of workspace.files.values()) {
      for (const section of other.sections) {
        const prefixed = lookup.sectionPrefix(section.name.text) !== undefined;
        // Global sections from included files merge into the main one instead of duplicating.
        if (other.namespace !== '' && !prefixed) continue;
        const key = section.qualifiedName.toLowerCase();
        const first = firstByName.get(key);
        if (!first) {
          firstByName.set(key, { path: other.path, line: section.header.line });
        } else if (other === file) {
          const where =
            first.path === file.path ? `line ${first.line + 1}` : `${first.path}:${first.line + 1}`;
          report(
            section.name.span.range,
            `Duplicate section found: [${section.name.text}] (first at ${where}; the DLL uses the first)`,
          );
        }
      }
    }
  },
};

/** Commas outside parentheses/brackets: the argument separators of a command value. */
function argumentCount(value: string): number {
  let depth = 0;
  let count = 1;
  for (const c of value) {
    if (c === '(' || c === '[') depth++;
    else if (c === ')' || c === ']') depth--;
    else if (c === ',' && depth === 0) count++;
  }
  return count;
}

export const XM108: Rule = {
  id: 'XM108',
  severity: 'error',
  description: 'Wrong argument count for a command (`store`, draw commands, …)',
  check({ file, lookup, report }) {
    for (const section of file.sections) {
      if (section.spec?.kind !== 'commandlist') continue;
      const errorLines = new Set(section.syntaxIssues.map((i) => i.range.start.line));
      for (const line of section.lines) {
        if (!line.value?.text || errorLines.has(line.line)) continue;
        const command = lookup.command(line.command ?? '');
        if (
          command?.argCount === undefined ||
          command.match !== 'exact' ||
          command.name !== line.command
        )
          continue;
        const value = line.value.text.toLowerCase();
        if (
          command.values.some((v) =>
            v.match === 'exact' ? value === v.name : value.startsWith(v.name),
          )
        )
          continue;
        const count = argumentCount(line.value.text);
        if (count !== command.argCount) {
          report(
            line.value.span.range,
            `\`${command.name}\` takes ${command.argCount} argument${command.argCount === 1 ? '' : 's'}, got ${count}`,
          );
        }
      }
    }
  },
};

/** Exposed for tests. */
export const internals = { argumentCount, isFreeForm };
export type { RuleContext };
