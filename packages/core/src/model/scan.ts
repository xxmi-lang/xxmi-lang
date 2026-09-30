/**
 * Splits an ini into preamble, sections and lines exactly as the DLL's `ParseIniStream` does
 * (IniHandler.cpp): lines are trimmed of spaces and tabs, blank lines are skipped, a `;` as the
 * first character makes a comment (a `;` later in the line is part of the value), and any line
 * starting with `[` is a section header whose name runs to the first `]`. Key/value lines split
 * at the first `=` (`ParseIniKeyValLine`).
 *
 * This is the structural backbone of the model: it can't be confused by a syntax error, so an
 * error in one section never hides the sections after it.
 */
import type { Position, Range } from '../parser/index.ts';

export interface TextSpan {
  /** UTF-16 offsets into the file text. */
  start: number;
  end: number;
  range: Range;
}

export interface ScannedText {
  text: string;
  span: TextSpan;
}

export interface ScannedLine {
  /** 0-based line number. */
  line: number;
  /** The trimmed line. */
  content: ScannedText;
  /** Present when the line contains `=`: the trimmed text before it. */
  key?: ScannedText;
  /** Present when the line contains `=`: the trimmed text after it (may be empty). */
  value?: ScannedText;
}

export interface ScannedSection {
  /** Name between `[` and the first `]` (or end of line), trimmed. */
  name: ScannedText;
  header: ScannedLine;
  lines: ScannedLine[];
  /** From the header's first character to the start of the next header (or end of file). */
  chunk: TextSpan;
}

export interface ScannedIni {
  preamble: ScannedLine[];
  /** From the start of the file to the first section header (or end of file). */
  preambleChunk: TextSpan;
  sections: ScannedSection[];
  comments: ScannedLine[];
}

class LineIndex {
  private readonly starts: number[] = [0];

  constructor(text: string) {
    for (let i = 0; i < text.length; i++) if (text[i] === '\n') this.starts.push(i + 1);
  }

  position(offset: number): Position {
    let lo = 0;
    let hi = this.starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if ((this.starts[mid] ?? 0) <= offset) lo = mid;
      else hi = mid - 1;
    }
    return { line: lo, character: offset - (this.starts[lo] ?? 0) };
  }
}

export function scanIni(text: string): ScannedIni {
  const index = new LineIndex(text);
  const span = (start: number, end: number): TextSpan => ({
    start,
    end,
    range: { start: index.position(start), end: index.position(end) },
  });
  const piece = (start: number, end: number): ScannedText => ({
    text: text.slice(start, end),
    span: span(start, end),
  });

  const preamble: ScannedLine[] = [];
  const sections: ScannedSection[] = [];
  const comments: ScannedLine[] = [];
  let current: ScannedSection | undefined;
  let firstHeader = text.length;

  let lineNo = 0;
  for (let lineStart = 0; lineStart <= text.length; lineNo++) {
    let lineEnd = text.indexOf('\n', lineStart);
    if (lineEnd < 0) lineEnd = text.length;
    let start = lineStart;
    let end = lineEnd;
    if (text[end - 1] === '\r') end--;
    while (start < end && (text[start] === ' ' || text[start] === '\t')) start++;
    while (end > start && (text[end - 1] === ' ' || text[end - 1] === '\t')) end--;
    const next = lineEnd + 1;

    if (start < end) {
      const line: ScannedLine = { line: lineNo, content: piece(start, end) };
      if (text[start] === ';') {
        comments.push(line);
      } else if (text[start] === '[') {
        const close = text.indexOf(']', start);
        let nameStart = start + 1;
        let nameEnd = close >= 0 && close < end ? close : end;
        while (nameStart < nameEnd && (text[nameStart] === ' ' || text[nameStart] === '\t'))
          nameStart++;
        while (nameEnd > nameStart && (text[nameEnd - 1] === ' ' || text[nameEnd - 1] === '\t'))
          nameEnd--;
        if (current) current.chunk = span(current.chunk.start, lineStart);
        else firstHeader = lineStart;
        current = {
          name: piece(nameStart, nameEnd),
          header: line,
          lines: [],
          chunk: span(lineStart, text.length),
        };
        sections.push(current);
      } else {
        const eq = text.indexOf('=', start);
        if (eq >= 0 && eq < end) {
          let keyEnd = eq;
          while (keyEnd > start && (text[keyEnd - 1] === ' ' || text[keyEnd - 1] === '\t'))
            keyEnd--;
          let valueStart = eq + 1;
          while (valueStart < end && (text[valueStart] === ' ' || text[valueStart] === '\t'))
            valueStart++;
          line.key = piece(start, keyEnd);
          line.value = piece(valueStart, end);
        }
        (current ? current.lines : preamble).push(line);
      }
    }
    if (lineEnd >= text.length) break;
    lineStart = next;
  }

  if (current) current.chunk = span(current.chunk.start, text.length);
  return { preamble, preambleChunk: span(0, firstHeader), sections, comments };
}
