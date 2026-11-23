import type { Range } from '../../parser/index.ts';
import type { LineModel } from '../../model/types.ts';
import type { Fix, Rule } from '../types.ts';

/**
 * Offset in `value` of a `;` that opens a comment: the first one at the start or after a blank.
 * A `;` glued to other text (`a;b.dds`) is left alone.
 */
export function commentStart(value: string): number | undefined {
  const match = /(^|[ \t]);/.exec(value);
  return match ? match.index + (match[1]?.length ?? 0) : undefined;
}

/**
 * Replaces the line with its comment, then the code, keeping the indent.
 * A line that is only a comment after the `=` has no code to keep, so it gets no fix.
 */
function moveAbove(text: string, line: LineModel, at: number): Fix | undefined {
  const { start, end } = line.content.span;
  const code = text.slice(start, at).replace(/[ \t]+$/, '');
  if (code.endsWith('=')) return undefined;

  const indent = text.slice(text.lastIndexOf('\n', start - 1) + 1, start);
  return {
    title: 'Move the comment above the line',
    // A plain \n keeps the edit identical for LF and CRLF files; editors normalise it on insert.
    edits: [
      {
        range: line.content.span.range,
        newText: `${text.slice(at, end)}\n${indent}${code}`,
      },
    ],
  };
}

export const XM115: Rule = {
  id: 'XM115',
  severity: 'warning',
  description: '`;` comment after a value (the DLL keeps it in the value)',
  check({ file, report }) {
    for (const section of file.sections) {
      for (const line of section.lines) {
        if (!line.value) continue;
        const offset = commentStart(line.value.text);
        if (offset === undefined) continue;

        const { start, end } = line.value.span.range;
        const range: Range = { start: { ...start, character: start.character + offset }, end };
        // ParseIniStream: "a semicolon appearing in the middle of a line is *NOT* a comment".
        report(
          range,
          "`;` after a value doesn't start a comment: the DLL keeps it in the value.",
          moveAbove(file.text, line, line.value.span.start + offset),
        );
      }
    }
  },
};
