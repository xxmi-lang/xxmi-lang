import type {
  CommandKind,
  SpecCommand,
  SpecCommandValue,
  SpecFunction,
  SpecResourceMember,
} from '@xxmi-lang/core';
import {
  AnchorNotFoundError,
  CppFile,
  findCalls,
  ifBlocks,
  splitTopLevel,
  stringLiteral,
} from './cpp.ts';
import type { Extraction } from './dll.ts';

const CL = 'CommandList.cpp';
const INI = 'IniHandler.cpp';

interface Match {
  name: string;
  match: 'exact' | 'prefix';
}

/**
 * Literal comparisons against `subject` in `code`: `!wcscmp(subject, L"x")` is exact,
 * `!wcsncmp(subject, L"x", n)` and `subject.compare(0, n, L"x")` are prefix matches.
 */
function comparisons(code: string, subject: RegExp): Match[] {
  const s = subject.source;
  const out: Match[] = [];
  const pattern = new RegExp(
    `wcs(n)?cmp\\s*\\(\\s*${s}\\s*,\\s*L"([^"]+)"|${s}\\s*(?:\\.|->)\\s*compare\\s*\\(\\s*0\\s*,\\s*\\d+\\s*,\\s*L"([^"]+)"`,
    'g',
  );
  for (const m of code.matchAll(pattern)) {
    if (m[2] !== undefined) out.push({ name: m[2], match: m[1] ? 'prefix' : 'exact' });
    else if (m[3] !== undefined) out.push({ name: m[3], match: 'prefix' });
  }
  return out;
}

const KEY = /key/;
const VALUE = /val->c_str\(\)/;
const LINE = /line->c_str\(\)/;

function command(
  m: Match,
  kind: CommandKind,
  source: string,
  values: SpecCommandValue[] = [],
): SpecCommand {
  return { name: m.name.trim(), kind, match: m.match, values, source, xxmi: false };
}

function values(code: string, source: string): SpecCommandValue[] {
  const out: SpecCommandValue[] = [];
  for (const v of comparisons(code, VALUE)) {
    if (!out.some((o) => o.name === v.name)) out.push({ ...v, source, xxmi: false });
  }
  return out;
}

/** `ParseCommandListGeneralCommands`: one if-block per command keyword. */
function generalCommands(file: CppFile): SpecCommand[] {
  const fn = 'ParseCommandListGeneralCommands';
  const source = `${file.name}:${fn}`;
  const commands: SpecCommand[] = [];
  for (const block of ifBlocks(file.functionBody(fn))) {
    for (const key of comparisons(block.condition, KEY)) {
      const existing = commands.find((c) => c.name === key.name);
      const vals = values(block.body, source);
      if (existing) {
        for (const v of vals)
          if (!existing.values.some((e) => e.name === v.name)) existing.values.push(v);
      } else {
        commands.push(command(key, 'general', source, vals));
      }
    }
  }
  if (commands.length === 0) throw new AnchorNotFoundError(file.name, fn, 'no key comparisons');
  return commands;
}

/** `ParseDrawCommand`: keyword, special values, and `ParseDrawCommandArgs(…, indirect, nargs, …)`. */
function drawCommands(file: CppFile): SpecCommand[] {
  const fn = 'ParseDrawCommand';
  const source = `${file.name}:${fn}`;
  const commands: SpecCommand[] = [];
  for (const block of ifBlocks(file.functionBody(fn))) {
    const [key] = comparisons(block.condition, KEY);
    if (!key) continue;
    const cmd = command(key, 'draw', source, values(block.body, source));
    const [args] = findCalls(block.body, 'ParseDrawCommandArgs');
    if (args) {
      const indirect = args.args[3];
      const count = Number(args.args[4]);
      if (Number.isInteger(count)) cmd.argCount = count;
      if (indirect === 'true') cmd.indirect = true;
    }
    commands.push(cmd);
  }
  if (commands.length === 0) throw new AnchorNotFoundError(file.name, fn, 'no draw keywords');
  return commands;
}

/** `ParseCommandListFlowControl`: `if `, `elif `, `else if `, `else`, `endif`. */
function flowControl(file: CppFile): SpecCommand[] {
  const fn = 'ParseCommandListFlowControl';
  const found = comparisons(file.functionBody(fn), LINE);
  if (found.length === 0) throw new AnchorNotFoundError(file.name, fn, 'no line comparisons');
  return found.map((m) => command(m, 'flow', `${file.name}:${fn}`));
}

/** `local` in `ParseCommandListVariableAssignment`, plus the `VariableFlagNames` keywords. */
function declarations(x: Extraction): SpecCommand[] {
  const out: SpecCommand[] = [];
  out.push(
    ...x.step([], () => {
      const fn = 'ParseCommandListVariableAssignment';
      // XXMI compares `line.compare(0, 5, L"local")`, vanilla `name.compare(0, 6, L"local ")`.
      const found = comparisons(x.dll.file(CL).functionBody(fn), /\w+/)
        .filter((m) => m.name.trim() === 'local')
        .slice(0, 1);
      if (found.length === 0) throw new AnchorNotFoundError(CL, fn, 'no `local` comparison');
      return found.map((m) => command(m, 'declaration', `${CL}:${fn}`));
    }),
  );
  out.push(
    ...x.step([], () => {
      const table = x.dll.file('CommandList.h').arrayInitializer('VariableFlagNames');
      return [...table.matchAll(/\{\s*L"([^"]+)"/g)].map((m) =>
        command(
          { name: m[1] ?? '', match: 'exact' },
          'declaration',
          'CommandList.h:VariableFlagNames',
        ),
      );
    }),
  );
  return out;
}

/** `pre ` / `post ` key prefixes in `ParseCommandList`. */
function prefixes(file: CppFile): SpecCommand[] {
  const fn = 'ParseCommandList';
  const found = comparisons(file.functionBody(fn), /key/).filter((m) =>
    /^(pre|post) $/.test(m.name),
  );
  if (found.length === 0) throw new AnchorNotFoundError(file.name, fn, 'no pre/post prefixes');
  return found.map((m) => command(m, 'prefix', `${file.name}:${fn}`));
}

/** The `members[]` table in `ResourceCopyTarget::ParseTargetMember` (`->Region(…)` etc.). */
function resourceMembers(file: CppFile): SpecResourceMember[] {
  const fn = 'ResourceCopyTarget::ParseTargetMember';
  const body = new CppFile(`${file.name}#${fn}`, file.functionBody(fn));
  const table = body.arrayInitializer('members');
  const members: SpecResourceMember[] = [];
  for (const entry of splitTopLevel(table)) {
    const inner = entry.replace(/^\{|\}$/g, '');
    const name = stringLiteral(splitTopLevel(inner)[0] ?? '');
    if (name === undefined) continue;
    const args = [...inner.matchAll(/MemberArg::Type::(\w+)/g)].map((m) =>
      (m[1] ?? '').toLowerCase(),
    );
    members.push({
      name: name.replace(/^->/, ''),
      args,
      source: `${file.name}:${fn}`,
      xxmi: false,
    });
  }
  if (members.length === 0) throw new AnchorNotFoundError(file.name, fn, 'empty members[] table');
  return members;
}

/** Function-call operators (`sin(…)`, `saturate(…)`, …) in the `unary_operators` group. */
export function functions(file: CppFile, patterns: Map<string, string>): SpecFunction[] {
  const group = 'unary_operators';
  return splitTopLevel(file.arrayInitializer(group))
    .map((ref) => patterns.get(ref.replace(/^&/, '')))
    .filter((p): p is string => p !== undefined && /^[a-z_]\w*$/i.test(p))
    .map((name) => ({ name, source: `${file.name}:${group}`, xxmi: false }));
}

export function extractCommands(x: Extraction): {
  commands: SpecCommand[];
  resourceMembers: SpecResourceMember[];
} {
  const cl = x.dll.file(CL);
  const ini = x.dll.file(INI);
  const general = generalCommands(cl);
  const draw = drawCommands(cl);
  // ParseCommandListGeneralCommands falls through to ParseDrawCommand for anything else, so a
  // draw keyword is never also a general one; keep the general entry if both ever match.
  const commands = [
    ...general,
    ...draw.filter((d) => !general.some((g) => g.name === d.name)),
    ...flowControl(cl),
    ...declarations(x),
    ...prefixes(ini),
  ];
  return { commands, resourceMembers: x.step([], () => resourceMembers(cl)) };
}
