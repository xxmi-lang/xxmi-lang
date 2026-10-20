/**
 * Doc comments of library symbols (docs/01-language-model.md "Doc comments"): the contiguous
 * `;` block directly above a section header or a `global` line, optionally with the structured
 * `;;` tags (`@desc`, `@in`, `@out`, `@since`, `@example`).
 */

export interface DocParam {
  name: string;
  description: string;
}

export interface DocComment {
  /** 1-based line of the first comment line. */
  line: number;
  /** 1-based line of the last comment line. */
  endLine: number;
  /** The comment text with the `;` markers removed. */
  text: string;
  /** Present when the block uses the structured `;;` convention. */
  tags?: {
    desc?: string;
    since?: string;
    in: DocParam[];
    out: DocParam[];
    example?: string;
  };
}

/** `;;`-prefixed line? */
const isStructured = (line: string): boolean => line.trimStart().startsWith(';;');

function stripMarker(line: string): string {
  return line.trimStart().replace(/^;+[ \t]?/, '');
}

function isComment(line: string | undefined): boolean {
  return line !== undefined && line.trimStart().startsWith(';');
}

/** Comment lines ending at 0-based `end` (inclusive), or undefined when line `end` isn't one. */
function blockEndingAt(
  lines: string[],
  end: number,
): { start: number; lines: string[] } | undefined {
  if (!isComment(lines[end])) return undefined;
  let start = end;
  while (start > 0 && isComment(lines[start - 1])) start--;
  return { start, lines: lines.slice(start, end + 1) };
}

function parseParam(rest: string): DocParam {
  const match = /^(\S+)\s*(.*)$/s.exec(rest.trim());
  return { name: match?.[1] ?? '', description: match?.[2]?.trim() ?? '' };
}

function parseTags(lines: string[]): NonNullable<DocComment['tags']> {
  const tags: NonNullable<DocComment['tags']> = { in: [], out: [] };
  const example: string[] = [];
  let inExample = false;
  for (const raw of lines) {
    const text = stripMarker(raw);
    const tag = /^@(\w+)\s*(.*)$/.exec(text.trim());
    if (!tag) {
      if (inExample) example.push(text);
      continue;
    }
    const [, name = '', rest = ''] = tag;
    inExample = name === 'example';
    if (name === 'desc') tags.desc = rest;
    else if (name === 'since') tags.since = rest;
    else if (name === 'in') tags.in.push(parseParam(rest));
    else if (name === 'out') tags.out.push(parseParam(rest));
  }
  if (example.length > 0) tags.example = example.join('\n').trimEnd();
  return tags;
}

/** Doc comment ending directly above 0-based `line` (a section header or a declaration). */
export function docAbove(fileLines: string[], line: number): DocComment | undefined {
  const block = blockEndingAt(fileLines, line - 1);
  return block ? toDoc(block) : undefined;
}

function toDoc(block: { start: number; lines: string[] }): DocComment {
  const text = block.lines.map(stripMarker).join('\n').trimEnd();
  if (!block.lines.some(isStructured))
    return { line: block.start + 1, endLine: block.start + block.lines.length, text };
  const tags = parseTags(block.lines);
  return {
    line: block.start + 1,
    endLine: block.start + block.lines.length,
    text: tags.desc ?? text,
    tags,
  };
}

/** Every contiguous comment block of a file, in order. */
export function commentBlocks(fileLines: string[]): DocComment[] {
  const blocks: DocComment[] = [];
  for (let i = 0; i < fileLines.length; i++) {
    if (!isComment(fileLines[i]) || isComment(fileLines[i + 1])) continue;
    const block = blockEndingAt(fileLines, i);
    if (block) blocks.push(toDoc(block));
  }
  return blocks;
}

export function splitLines(text: string): string[] {
  return text.split(/\r?\n/);
}
