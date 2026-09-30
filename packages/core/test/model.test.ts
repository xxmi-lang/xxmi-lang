import { describe, expect, it } from 'vitest';
import { loadSpec, lowerIni, scanIni, SpecLookup } from '../src/index.ts';

const lookup = new SpecLookup(loadSpec());
const lower = (text: string, defaultNamespace = 'mod.ini') =>
  lowerIni({ path: 'mod.ini', text, encoding: 'utf8', defaultNamespace, lookup });

describe('scanIni follows ParseIniStream', () => {
  it('splits sections, comments, key/value and bare lines', () => {
    const scanned = scanIni(
      'namespace = N\n; c\n[ A ] trailing\n  key = a = b  \n\tif $x\n\n[B\nx=1\r\n',
    );
    expect(scanned.preamble.map((l) => [l.key?.text, l.value?.text])).toEqual([['namespace', 'N']]);
    expect(scanned.comments.map((c) => c.content.text)).toEqual(['; c']);
    expect(scanned.sections.map((s) => s.name.text)).toEqual(['A', 'B']);
    const [kv, bare] = scanned.sections[0]?.lines ?? [];
    expect([kv?.key?.text, kv?.value?.text]).toEqual(['key', 'a = b']);
    expect(bare?.key).toBeUndefined();
    expect(bare?.content.text).toBe('if $x');
    expect(scanned.sections[1]?.lines[0]?.value?.text).toBe('1');
  });

  it('treats `;` as a comment only at the start of a line', () => {
    const [line] = scanIni('[A]\nrun = CommandListB ; not a comment\n').sections[0]?.lines ?? [];
    expect(line?.value?.text).toBe('CommandListB ; not a comment');
  });
});

describe('lowerIni', () => {
  it('keeps later sections when a section has a syntax error', async () => {
    const file = await lower('[CommandListA]\n$y = ($x + 1\n$z = 2\n\n[CommandListB]\n$w = 3\n');
    expect(file.sections.map((s) => s.name.text)).toEqual(['CommandListA', 'CommandListB']);
    expect(file.sections[0]?.syntaxIssues.length).toBeGreaterThan(0);
    expect(file.sections[1]?.syntaxIssues).toEqual([]);
    expect(file.references.filter((r) => r.section === 1).map((r) => r.text)).toEqual(['$w']);
  });

  it('namespaces prefixed sections the way the DLL stores them', async () => {
    const file = await lower(
      'namespace = Lib\n[CommandList.Set]\n[Constants]\n[ResourceX]\n',
      'ignored.ini',
    );
    expect(file.namespace).toBe('Lib');
    expect(file.sections.map((s) => s.qualifiedName)).toEqual([
      'CommandList\\Lib\\.Set',
      'Constants',
      'Resource\\Lib\\X',
    ]);
    expect(file.sections.map((s) => s.kind)).toEqual(['commandlist', undefined, 'resource']);
  });

  it('leaves names alone in d3dx.ini (no namespace)', async () => {
    const file = await lower('[CommandListA]\n', '');
    expect(file.sections[0]?.qualifiedName).toBe('CommandListA');
  });

  it('collects references with their access', async () => {
    const file = await lower(
      [
        '[CommandListA]',
        'post run = CommandList\\Lib\\Draw',
        'ResourceOut = copy ResourceIn',
        '$PoolT[$i] = $v',
        'store = $packed, ResourceBuf, 7',
        'local $n = PoolT->Size',
      ].join('\n'),
    );
    expect(file.references.map((r) => `${r.kind}:${r.text}:${r.access}`)).toEqual([
      'commandlist:CommandList\\Lib\\Draw:read',
      'resource:ResourceOut:write',
      'resource:ResourceIn:read',
      'pool:PoolT:write',
      'variable:$i:read',
      'variable:$v:read',
      'variable:$packed:write',
      'resource:ResourceBuf:read',
      'pool:PoolT:read',
    ]);
    expect(file.variables.map((v) => `${v.scope}:${v.name}`)).toEqual(['local:$n']);
    const run = file.sections[0]?.lines[0];
    expect([run?.modifier, run?.command]).toEqual(['post', 'run']);
  });

  it('records global flags', async () => {
    const file = await lower(
      '[Constants]\nglobal persist $a = 1\nglobal locked $b = 2\nglobal $c\n',
    );
    expect(file.variables.map((v) => [v.name, v.persist, v.locked])).toEqual([
      ['$a', true, false],
      ['$b', false, true],
      ['$c', false, false],
    ]);
  });
});
