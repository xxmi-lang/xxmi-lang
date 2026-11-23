/**
 * Structure rules (XM1xx). Messages quote the DLL's own warning text where it has one
 * (IniHandler.cpp), so editor output matches the game log.
 */
import { dirname, relative, sep } from 'node:path';
import type { IniFile, LineModel, SectionModel } from '../../model/types.ts';
import { keyCommands } from '../../spec/lookup.ts';
import type { SpecKey } from '../../spec/types.ts';
import {
  commentOutFix,
  didYouMean,
  editDistance,
  insertLineAfter,
  replaceFix,
} from '../suggest.ts';
import type { Fix, Rule, RuleContext } from '../types.ts';
import { compareLoadOrder, relativeToRoot } from '../../workspace/load-order.ts';
import { gradeDuplicate } from './duplicate-grade.ts';

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

/**
 * A flow keyword where the DLL doesn't read flow control: the preamble (no section)
 * or a section that isn't a command list (`ParseCommandList` only runs on those).
 * Unknown sections are left to XM101.
 */
export function isMisplacedFlow(section: SectionModel | undefined, line: LineModel): boolean {
  const outside = section === undefined || (section.spec && section.spec.kind !== 'commandlist');
  return Boolean(outside) && flowKeyword(line) !== undefined;
}

/**
 * `ParseIniKeyValLine` warns "Malformed line" and drops it.
 * XM111 owns a stray `else` or `endif`.
 */
export function isMalformedLine(section: SectionModel, line: LineModel): boolean {
  const strict = section.spec !== undefined && !section.spec.allowsBareLines;
  return strict && !line.key && !isMisplacedFlow(section, line);
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
        // XM111 owns `if $x == 1` and the like.
        if (!line.key || isMisplacedFlow(section, line)) continue;
        const key = line.key.text;
        if (spec.kind === 'commandlist') {
          // The grammar parses anything it doesn't recognise as a command, resource copy or
          // assignment as a generic `key = value` setting.
          if (line.statement !== 'setting_statement') continue;
          const command = line.command ?? key.toLowerCase();
          if (lookup.key(spec, command) || lookup.command(command)) continue;
          report(
            line.content.span.range,
            `Unrecognised entry: ${line.content.text}.${didYouMean(command, [...known, ...keyCommands(lookup.spec).map((c) => c.name)])}`,
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
      for (const line of section.lines) {
        if (!isMalformedLine(section, line)) continue;
        const range = line.content.span.range;
        report(range, `Malformed line: "${line.content.text}"`, commentOutFix(range));
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

/** Adds `endif` after the section's last command, indented like the `if` it closes. */
function insertEndif(text: string, section: SectionModel, opening: LineModel): Fix {
  const last = section.lines.at(-1) ?? opening;
  return {
    title: 'Add the missing `endif`',
    edits: [insertLineAfter(text, last, opening, 'endif')],
  };
}

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
          insertEndif(file.text, section, line),
        );
      }
    }
  },
};

/** Relative to the duplicate's folder, so the message doesn't depend on where the package lives. */
function relativeLocation(from: IniFile, to: IniFile): string {
  return relative(dirname(from.path), to.path).split(sep).join('/');
}

export const XM107: Rule = {
  id: 'XM107',
  severity: 'warning',
  description:
    'Duplicate section name within the same namespace (error, warning or info per finding)',
  check({ file, workspace, lookup, report }) {
    // The DLL keeps the first and warns for the rest, in its own load order (see load-order.ts).
    // Files it never loads (`exclude_recursive`, e.g. DISABLED folders) can only clash with
    // themselves: a disabled copy of a library isn't a duplicate of the enabled one.
    const winners = new Map<string, { file: IniFile; section: SectionModel }>();
    const excluded = workspace.isExcluded(file.path);
    const ownPackage = workspace.packageRoot(file);
    const loadPath = (f: IniFile): string => relativeToRoot(f.path, ownPackage);
    const clashes = (other: IniFile, section: SectionModel): boolean =>
      // Global sections from included files merge into the main one instead of duplicating.
      other.namespace === '' || lookup.sectionPrefix(section.name.text) !== undefined;

    for (const other of workspace.files.values()) {
      if (other !== file && (excluded || workspace.isExcluded(other.path))) continue;
      // One lint run can span several packages; the DLL only ever loads one.
      if (workspace.packageRoot(other) !== ownPackage) continue;
      for (const section of other.sections) {
        if (!clashes(other, section)) continue;
        const key = section.qualifiedName.toLowerCase();
        const first = winners.get(key);
        if (
          !first ||
          (other !== first.file && compareLoadOrder(loadPath(other), loadPath(first.file)) < 0)
        ) {
          winners.set(key, { file: other, section });
        }
      }
    }

    for (const section of file.sections) {
      if (!clashes(file, section)) continue;
      const first = winners.get(section.qualifiedName.toLowerCase());
      if (!first || first.section === section) continue;

      const line = first.section.header.line + 1;
      const sameFile = first.file === file;
      const where = sameFile ? `line ${line}` : `${relativeLocation(file, first.file)}:${line}`;
      const grade = sameFile ? undefined : gradeDuplicate(workspace, { file, section }, first);
      const note = grade?.note ? `; ${grade.note}` : '';
      report(
        section.name.span.range,
        `Duplicate section found: [${section.name.text}] (first at ${where}; the DLL uses the first${note})`,
        undefined,
        grade?.severity,
      );
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
