/** "Did you mean …" suggestions by edit distance. */

export function editDistance(a: string, b: string): number {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  let prev = Array.from({ length: y.length + 1 }, (_, i) => i);
  for (let i = 1; i <= x.length; i++) {
    const row = [i];
    for (let j = 1; j <= y.length; j++) {
      const cost = x[i - 1] === y[j - 1] ? 0 : 1;
      row[j] = Math.min((prev[j] ?? 0) + 1, (row[j - 1] ?? 0) + 1, (prev[j - 1] ?? 0) + cost);
    }
    prev = row;
  }
  return prev[y.length] ?? 0;
}

/** The closest candidate within a distance that scales with length, or undefined. */
export function nearest(word: string, candidates: Iterable<string>): string | undefined {
  const limit = Math.max(1, Math.min(3, Math.floor(word.length / 3)));
  let best: string | undefined;
  let bestDistance = limit + 1;
  for (const candidate of candidates) {
    const d = editDistance(word, candidate);
    if (d < bestDistance || (d === bestDistance && best !== undefined && candidate < best)) {
      best = candidate;
      bestDistance = d;
    }
  }
  return bestDistance <= limit ? best : undefined;
}

export function didYouMean(word: string, candidates: Iterable<string>): string {
  const match = nearest(word, candidates);
  return match ? ` Did you mean \`${match}\`?` : '';
}

/** A "comment out the line" quick fix, for lines the DLL warns about and drops. */
export function commentOutFix(range: import('../parser/index.ts').Range): import('./types.ts').Fix {
  const { start } = range;
  return {
    title: 'Comment out the line',
    edits: [{ range: { start, end: start }, newText: '; ' }],
  };
}

type Edit = import('./types.ts').Fix['edits'][number];
type Line = import('../model/types.ts').LineModel;

/** Start offset of the line holding `offset`, and the text before `offset` on it. */
export function lineStartIndent(text: string, offset: number): { start: number; indent: string } {
  const start = text.lastIndexOf('\n', offset - 1) + 1;
  return { start, indent: text.slice(start, offset) };
}

/**
 * The line break an edit on the line holding `offset` should insert, so CRLF files stay CRLF.
 * A last line without a terminator takes the file's first break.
 */
export function lineEnding(text: string, offset: number): '\r\n' | '\n' {
  const found = text.indexOf('\n', offset);
  const newline = found === -1 ? text.indexOf('\n') : found;
  return newline > 0 && text[newline - 1] === '\r' ? '\r\n' : '\n';
}

/** An insertion of `body` as a new line above `line`, with its indent. */
export function insertLineAbove(text: string, line: Line, body: string): Edit {
  const { start } = line.content.span;
  const { indent } = lineStartIndent(text, start);
  const at = { line: line.line, character: 0 };
  return { range: { start: at, end: at }, newText: `${indent}${body}${lineEnding(text, start)}` };
}

/** An insertion of `body` as a new line after `last`, indented like `like`. */
export function insertLineAfter(text: string, last: Line, like: Line, body: string): Edit {
  const at = last.content.span.range.end;
  const { indent } = lineStartIndent(text, like.content.span.start);
  const eol = lineEnding(text, last.content.span.end);
  return { range: { start: at, end: at }, newText: `${eol}${indent}${body}` };
}

/** A "replace with the suggestion" quick fix, if there is a suggestion. */
export function replaceFix(
  range: import('../parser/index.ts').Range,
  word: string,
  candidates: Iterable<string>,
  prefix = '',
): import('./types.ts').Fix | undefined {
  const match = nearest(word, candidates);
  return match
    ? { title: `Change to \`${prefix}${match}\``, edits: [{ range, newText: prefix + match }] }
    : undefined;
}
