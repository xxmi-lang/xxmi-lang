import type { LineModel, SectionModel } from '../../model/types.ts';
import type { SpecSection } from '../../spec/types.ts';
import { insertLineAbove } from '../suggest.ts';
import type { Fix, Rule } from '../types.ts';
import { flowKeyword } from './structure.ts';

const OVERRIDE_SECTIONS = new Set(['TextureOverride', 'ShaderOverride']);

/** A setting line found in one branch of an `if` chain. */
interface Setting {
  key: string;
  value: string;
  line: LineModel;
  /** Index of its report. */
  report: number;
}

interface Chain {
  ifLine: LineModel;
  hasElse: boolean;
  branches: Setting[][];
}

interface Pending {
  line: LineModel;
  message: string;
  fix?: Fix;
}

/**
 * The line as a setting of the section;
 * ParseCommandList skips these keys before it reads flow control,
 * and the GetIni* readers never look at `if` (IniHandler.cpp).
 */
function settingOf(spec: SpecSection, line: LineModel, report: number): Setting | undefined {
  const key = line.key?.text.toLowerCase();
  if (key === undefined || !spec.keys.some((known) => known.name === key)) return undefined;
  return { key, value: line.value?.text ?? '', line, report };
}

function onlyCopy(branch: Setting[], wanted: Setting): Setting | undefined {
  const sameKey = branch.filter((s) => s.key === wanted.key);
  const [copy] = sameKey;
  return sameKey.length === 1 && copy?.value === wanted.value ? copy : undefined;
}

function hoistFix(text: string, ifLine: LineModel, first: Setting, copies: Setting[]): Fix {
  return {
    title: `Move \`${first.key}\` above the \`if\``,
    edits: [
      insertLineAbove(text, ifLine, first.line.content.text),
      ...copies.map((copy) => ({
        range: {
          start: { line: copy.line.line, character: 0 },
          end: { line: copy.line.line + 1, character: 0 },
        },
        newText: '',
      })),
    ],
  };
}

/**
 * A setting that appears once, with the same value, in every branch of a chain that has an `else`
 * is set the same whichever branch runs, so one copy above the `if` replaces them all.
 * Only the first copy's report carries the fix, so applying every fix doesn't repeat it.
 */
function attachHoists(text: string, chain: Chain, pending: Pending[]): void {
  if (!chain.hasElse) return;

  for (const setting of chain.branches[0] ?? []) {
    const copies = chain.branches.map((branch) => onlyCopy(branch, setting));
    const found = copies.filter((copy): copy is Setting => copy !== undefined);
    const target = pending[setting.report];
    if (found.length === copies.length && target) {
      target.fix = hoistFix(text, chain.ifLine, setting, found);
    }
  }
}

function startElse(chain: Chain): void {
  chain.hasElse = true;
  chain.branches.push([]);
}

function flagSetting(spec: SpecSection, line: LineModel, chain: Chain, pending: Pending[]): void {
  const setting = settingOf(spec, line, pending.length);
  if (!setting) return;

  pending.push({
    line,
    message: `\`${setting.key}\` inside an \`if\` is not conditional: the DLL reads it as a section setting, whatever the \`if\` says.`,
  });
  chain.branches.at(-1)?.push(setting);
}

function scanSection(
  text: string,
  section: SectionModel,
  spec: SpecSection,
  pending: Pending[],
): void {
  const open: Chain[] = [];
  for (const line of section.lines) {
    const keyword = flowKeyword(line);
    const top = open.at(-1);
    if (keyword === 'if') open.push({ ifLine: line, hasElse: false, branches: [[]] });
    else if (keyword === 'elif' && top) top.branches.push([]);
    else if (keyword === 'else' && top) startElse(top);
    else if (keyword === 'endif' && top) {
      open.pop();
      attachHoists(text, top, pending);
    } else if (!keyword && top) flagSetting(spec, line, top, pending);
  }
}

export const XM114: Rule = {
  id: 'XM114',
  severity: 'info',
  description:
    'TextureOverride / ShaderOverride setting written inside an `if` (the DLL reads it unconditionally)',
  check({ file, report }) {
    for (const section of file.sections) {
      const spec = section.spec;
      if (!spec || !OVERRIDE_SECTIONS.has(spec.name)) continue;

      const pending: Pending[] = [];
      scanSection(file.text, section, spec, pending);
      for (const { line, message, fix } of pending) {
        report(line.content.span.range, message, fix);
      }
    }
  },
};
