import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { decodeText, initParser, parseFile, parseText } from '../src/index.ts';

const FIXTURES = resolve(import.meta.dirname, '../../../fixtures/parser');
const GRAMMAR = resolve(import.meta.dirname, '../grammar');
const UPDATE = process.env.UPDATE_GOLDEN === '1';

function listIni(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true, recursive: true })
    .filter((e) => e.isFile() && e.name.endsWith('.ini'))
    .map((e) => join(e.parentPath, e.name))
    .sort();
}

describe('golden parser fixtures', () => {
  // Each fixtures/parser/**/<case>.ini has <case>.expected.json with its encoding and issues.
  // Regenerate with UPDATE_GOLDEN=1 pnpm test, then review the diff.
  for (const file of listIni(FIXTURES)) {
    const name = relative(FIXTURES, file).replaceAll('\\', '/');
    it(name, async () => {
      const result = await parseFile(file);
      const actual = { encoding: result.encoding, issues: result.issues };
      result.tree.delete();

      const expectedPath = file.replace(/\.ini$/, '.expected.json');
      if (UPDATE) writeFileSync(expectedPath, `${JSON.stringify(actual, null, 2)}\n`);
      expect(existsSync(expectedPath), `missing ${expectedPath}`).toBe(true);
      expect(actual).toEqual(JSON.parse(readFileSync(expectedPath, 'utf8')));
    });
  }

  it('xxmi-dialect fixtures parse without issues', async () => {
    for (const file of listIni(join(FIXTURES, 'xxmi-dialect'))) {
      const result = await parseFile(file);
      result.tree.delete();
      expect(result.issues, file).toEqual([]);
    }
  });

  it('errors fixtures each report at least one issue', async () => {
    for (const file of listIni(join(FIXTURES, 'errors'))) {
      const result = await parseFile(file);
      result.tree.delete();
      expect(result.issues.length, file).toBeGreaterThan(0);
    }
  });
});

describe('parseText', () => {
  it('reports ranges in UTF-16 code units', async () => {
    const { tree, issues } = await parseText('[CommandListA]\n$名前 = 2\n');
    tree.delete();
    expect(issues).toHaveLength(1);
    expect(issues[0]?.range).toEqual({
      start: { line: 1, character: 0 },
      end: { line: 1, character: 7 },
    });
    expect(issues[0]?.startOffset).toBe(15);
    expect(issues[0]?.endOffset).toBe(22);
  });

  async function sectionHeaders(text: string): Promise<string[]> {
    const { tree } = await parseText(text);
    const headers = tree.rootNode.namedChildren
      .filter((n) => n.type.endsWith('_section'))
      .map((n) => n.childForFieldName('header')?.text.trim() ?? '');
    tree.delete();
    return headers;
  }

  it('keeps the next section after an unterminated if', async () => {
    const text = '[CommandListA]\nif $x == 1\n\t$y = 2\n\n[CommandListB]\n$z = 3\n';
    expect(await sectionHeaders(text)).toContain('[CommandListB]');
  });

  // Known recovery gap: tree-sitter folds the rest of the file into one ERROR node.
  // docs/01-language-model.md §2 "Error recovery"; M2 lowering must work around it.
  it.fails('keeps the next section after an unclosed (', async () => {
    const text = '[CommandListA]\n$y = ($x + 1\n$z = 2\n\n[CommandListB]\n$w = 3\n';
    expect(await sectionHeaders(text)).toContain('[CommandListB]');
  });

  it('parses empty and whitespace-only input', async () => {
    for (const text of ['', '\n', '   \r\n\t\n', '; only a comment']) {
      const { tree, issues } = await parseText(text);
      tree.delete();
      expect(issues, JSON.stringify(text)).toEqual([]);
    }
  });

  it('never throws on arbitrary input', async () => {
    const inputs = ['[', ']]]', '[CommandList', '$$$ = = =', '\u0000\u0001', 'if\nelse\nendif\n'];
    for (const text of inputs) {
      const { tree } = await parseText(text);
      tree.delete();
    }
  });
});

describe('decodeText', () => {
  it('strips a UTF-8 BOM', () => {
    const bytes = new Uint8Array([0xef, 0xbb, 0xbf, 0x61]);
    expect(decodeText(bytes)).toEqual({ text: 'a', encoding: 'utf8bom' });
  });

  it('flags invalid UTF-8 as unknown and still decodes', () => {
    const bytes = new Uint8Array([0x3b, 0xd6, 0xd0, 0x0a, 0x61]);
    const { text, encoding } = decodeText(bytes);
    expect(encoding).toBe('unknown');
    expect(text.endsWith('\na')).toBe(true);
  });
});

describe('grammar', () => {
  it('SOURCE.json matches the vendored WASM', () => {
    const source = JSON.parse(readFileSync(join(GRAMMAR, 'SOURCE.json'), 'utf8')) as {
      sha256: string;
    };
    const wasm = readFileSync(join(GRAMMAR, 'tree-sitter-migoto.wasm'));
    expect(createHash('sha256').update(wasm).digest('hex')).toBe(source.sha256);
  });

  it('initParser is idempotent', async () => {
    expect(await initParser()).toBe(await initParser());
  });
});
