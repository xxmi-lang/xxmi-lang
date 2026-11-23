import type { Range } from '../../parser/index.ts';
import type { ScannedLine } from '../../model/types.ts';
import type { Rule } from '../types.ts';

/** Offset of the first `]` in a header, which is where `ParseIniSectionLine` ends the name. */
function closingBracket(header: ScannedLine): number {
  return header.content.text.indexOf(']');
}

/**
 * What follows the closing `]`, as the range to show and the range a fix removes. A `;` suffix is
 * not reported: the DLL ignores everything after the `]`, so a trailing comment changes nothing.
 */
function suffixAfterBracket(
  header: ScannedLine,
): { text: string; shown: Range; removed: Range } | undefined {
  const bracket = closingBracket(header);
  if (bracket < 0) return undefined;

  const { text } = header.content;
  const tail = text.slice(bracket + 1);
  const trimmed = tail.replace(/^[ \t]+/, '');
  if (trimmed === '' || trimmed.startsWith(';')) return undefined;

  const { start, end } = header.content.span.range;
  const at = (offset: number) => ({ ...start, character: start.character + offset });
  return {
    text: trimmed,
    shown: { start: at(text.length - trimmed.length), end },
    removed: { start: at(bracket + 1), end },
  };
}

/** Whether XM005 or XM006 reports this header, so XM001 leaves its line alone. */
export function isMalformedHeader(header: ScannedLine): boolean {
  return closingBracket(header) < 0 || suffixAfterBracket(header) !== undefined;
}

export const XM005: Rule = {
  id: 'XM005',
  severity: 'warning',
  description: 'Text after the `]` of a section header (the DLL ignores it)',
  check({ file, report }) {
    for (const section of file.sections) {
      const suffix = suffixAfterBracket(section.header);
      if (!suffix) continue;

      report(
        suffix.shown,
        `Text after \`]\` is ignored: \`${suffix.text}\` (the DLL reads the section name up to the first \`]\`).`,
        {
          title: 'Remove the text after `]`',
          edits: [{ range: suffix.removed, newText: '' }],
        },
      );
    }
  },
};

export const XM006: Rule = {
  id: 'XM006',
  severity: 'warning',
  description: 'Section header with no closing `]`',
  check({ file, report }) {
    for (const { header } of file.sections) {
      if (closingBracket(header) >= 0) continue;

      const { end } = header.content.span.range;
      report(
        header.content.span.range,
        'Section header has no closing `]`: the DLL takes the rest of the line as the name.',
        { title: 'Add the closing `]`', edits: [{ range: { start: end, end }, newText: ']' }] },
      );
    }
  },
};
