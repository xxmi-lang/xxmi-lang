/**
 * Inline suppression, written as plain `;` comments the DLL ignores (docs/02-diagnostics.md):
 *
 *   ; xxmi-disable-next-line XM204        next non-comment line only
 *   ; xxmi-disable XM401 … ; xxmi-enable XM401
 *   ; xxmi-disable                         all rules, until `; xxmi-enable`
 */
import type { IniFile } from '../model/types.ts';
import type { Diagnostic } from './types.ts';

const DIRECTIVE = /^;\s*xxmi-(disable-next-line|disable|enable)\b\s*(.*)$/i;

interface Region {
  ids: Set<string> | 'all';
  fromLine: number;
  toLine: number;
}

export function suppressionFilter(file: IniFile): (d: Diagnostic) => boolean {
  const regions: Region[] = [];
  const open = new Map<string, number>(); // rule id (or '*') → line the disable starts at
  const contentLines = new Set<number>([
    ...file.preamble.map((l) => l.line),
    ...file.sections.flatMap((s) => [s.header.line, ...s.lines.map((l) => l.line)]),
  ]);
  const nextContentLine = (after: number): number => {
    let line = after + 1;
    const last = file.text.split('\n').length;
    while (line <= last && !contentLines.has(line)) line++;
    return line;
  };

  for (const comment of file.comments) {
    const m = DIRECTIVE.exec(comment.content.text);
    if (!m) continue;
    const verb = (m[1] ?? '').toLowerCase();
    const list = (m[2] ?? '')
      .split(/[\s,]+/)
      .filter((id) => /^XM\d{3}$/i.test(id))
      .map((id) => id.toUpperCase());
    const ids: Set<string> | 'all' = list.length > 0 ? new Set(list) : 'all';
    if (verb === 'disable-next-line') {
      const line = nextContentLine(comment.line);
      regions.push({ ids, fromLine: line, toLine: line });
    } else if (verb === 'disable') {
      for (const id of ids === 'all' ? ['*'] : ids) if (!open.has(id)) open.set(id, comment.line);
    } else {
      for (const id of ids === 'all' ? [...open.keys()] : ids) {
        const from = open.get(id);
        if (from === undefined) continue;
        regions.push({
          ids: id === '*' ? 'all' : new Set([id]),
          fromLine: from,
          toLine: comment.line,
        });
        open.delete(id);
      }
    }
  }
  for (const [id, from] of open) {
    regions.push({
      ids: id === '*' ? 'all' : new Set([id]),
      fromLine: from,
      toLine: Number.MAX_SAFE_INTEGER,
    });
  }

  return (d) =>
    !regions.some(
      (r) =>
        d.range.start.line >= r.fromLine &&
        d.range.start.line <= r.toLine &&
        (r.ids === 'all' || r.ids.has(d.id)),
    );
}
