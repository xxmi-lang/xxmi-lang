/**
 * Just enough C++ reading to pull tables and call sites out of the DLL source. Everything is
 * located by symbol name, never by line number, so the extractor survives source drift.
 */

export class AnchorNotFoundError extends Error {
  readonly file: string;
  readonly symbol: string;

  constructor(file: string, symbol: string, detail = '') {
    super(`${file}: could not find ${symbol}${detail ? ` (${detail})` : ''}`);
    this.name = 'AnchorNotFoundError';
    this.file = file;
    this.symbol = symbol;
  }
}

/** A source file with comments blanked out (same length, newlines kept) for scanning. */
export class CppFile {
  readonly name: string;
  readonly text: string;
  readonly code: string;

  constructor(name: string, text: string) {
    this.name = name;
    this.text = text;
    this.code = stripComments(text);
  }

  /** Body (between the braces, exclusive) of the definition of `name`, e.g. `Foo::Bar`. */
  functionBody(name: string): string {
    const body = this.tryFunctionBody(name);
    if (body === undefined) throw new AnchorNotFoundError(this.name, `function ${name}`);
    return body;
  }

  tryFunctionBody(name: string): string | undefined {
    const pattern = new RegExp(`(?<![\\w.>:])${escapeRegExp(name)}\\s*\\(`, 'g');
    for (const match of this.code.matchAll(pattern)) {
      const open = match.index + match[0].length - 1;
      const close = matchBracket(this.code, open);
      if (close < 0) continue;
      let i = skipSpace(this.code, close + 1);
      // Skip trailing qualifiers such as `const` or `override`.
      for (let m = /^(const|override|noexcept)\b/.exec(this.code.slice(i)); m;) {
        i = skipSpace(this.code, i + m[0].length);
        m = /^(const|override|noexcept)\b/.exec(this.code.slice(i));
      }
      if (this.code[i] !== '{') continue;
      const end = matchBracket(this.code, i);
      if (end < 0) continue;
      return this.code.slice(i + 1, end);
    }
    return undefined;
  }

  /** Contents of the initializer of `name[] = { … }` (exclusive of the outer braces). */
  arrayInitializer(name: string): string {
    const body = this.tryArrayInitializer(name);
    if (body === undefined) throw new AnchorNotFoundError(this.name, `array ${name}[]`);
    return body;
  }

  tryArrayInitializer(name: string): string | undefined {
    const pattern = new RegExp(`\\b${escapeRegExp(name)}\\s*\\[\\s*\\]\\s*=\\s*\\{`);
    const match = pattern.exec(this.code);
    if (!match) return undefined;
    const open = match.index + match[0].length - 1;
    const close = matchBracket(this.code, open);
    return close < 0 ? undefined : this.code.slice(open + 1, close);
  }

  /** Replacement text of a (possibly multi-line) `#define NAME …`. */
  macro(name: string): string | undefined {
    const lines = this.code.split(/\r?\n/);
    const define = new RegExp(`^[ \\t]*#define[ \\t]+${escapeRegExp(name)}\\b(.*)$`);
    const start = lines.findIndex((l) => define.test(l));
    if (start < 0) return undefined;
    const body: string[] = [];
    let line = define.exec(lines[start] ?? '')?.[1] ?? '';
    for (let i = start + 1; ; i++) {
      const continued = /\\\s*$/.test(line);
      body.push(line.replace(/\\\s*$/, ''));
      if (!continued || i >= lines.length) break;
      line = lines[i] ?? '';
    }
    return body.join('\n');
  }

  /** Names of all `EnumName_t<…> NameNames[] = {` tables in the file. */
  enumTableNames(): string[] {
    const names: string[] = [];
    for (const m of this.code.matchAll(/EnumName_t\s*<[^>]*>\s*(\w+)\s*\[\s*\]\s*=\s*\{/g)) {
      if (m[1]) names.push(m[1]);
    }
    return names;
  }
}

/** Replaces `//` and `/* *\/` comments with spaces, leaving string and char literals intact. */
export function stripComments(text: string): string {
  let out = '';
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    const next = text[i + 1];
    if (c === '"' || c === "'") {
      const end = literalEnd(text, i);
      out += text.slice(i, end);
      i = end;
    } else if (c === '/' && next === '/') {
      while (i < text.length && text[i] !== '\n') {
        out += ' ';
        i++;
      }
    } else if (c === '/' && next === '*') {
      const end = text.indexOf('*/', i + 2);
      const stop = end < 0 ? text.length : end + 2;
      out += text.slice(i, stop).replace(/[^\n]/g, ' ');
      i = stop;
    } else {
      out += c ?? '';
      i++;
    }
  }
  return out;
}

/** Index just past the string or char literal starting at `start`. */
function literalEnd(text: string, start: number): number {
  const quote = text[start];
  let i = start + 1;
  while (i < text.length && text[i] !== quote && text[i] !== '\n') {
    i += text[i] === '\\' ? 2 : 1;
  }
  return i + 1;
}

/** Index of the bracket closing the one at `open`, skipping literals; -1 if unbalanced. */
export function matchBracket(code: string, open: number): number {
  const pairs: Record<string, string> = { '(': ')', '[': ']', '{': '}' };
  const stack: string[] = [];
  for (let i = open; i < code.length; i++) {
    const c = code[i];
    if (c === undefined) break;
    if (c === '"' || c === "'") {
      i = literalEnd(code, i) - 1;
      continue;
    }
    const closer = pairs[c];
    if (closer) stack.push(closer);
    else if (c === stack.at(-1)) {
      stack.pop();
      if (stack.length === 0) return i;
    }
  }
  return -1;
}

function skipSpace(code: string, i: number): number {
  while (i < code.length && /\s/.test(code[i] ?? '')) i++;
  return i;
}

export function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Splits at top-level commas (outside brackets and literals); trims each part. */
export function splitTopLevel(code: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < code.length; i++) {
    const c = code[i];
    if (c === '"' || c === "'") {
      i = literalEnd(code, i) - 1;
    } else if (c === '(' || c === '[' || c === '{') {
      depth++;
    } else if (c === ')' || c === ']' || c === '}') {
      depth--;
    } else if (c === ',' && depth === 0) {
      parts.push(code.slice(start, i).trim());
      start = i + 1;
    }
  }
  const last = code.slice(start).trim();
  if (last) parts.push(last);
  return parts;
}

/** The value of a `L"…"` or `"…"` literal expression, or undefined if it isn't one. */
export function stringLiteral(expr: string): string | undefined {
  const m = /^L?"((?:[^"\\]|\\.)*)"$/.exec(expr.trim());
  return m?.[1]?.replace(/\\(.)/g, '$1');
}

/** All string literal values in `code`, in order. */
export function stringLiterals(code: string): string[] {
  return [...code.matchAll(/L?"((?:[^"\\\n]|\\.)*)"/g)].map((m) =>
    (m[1] ?? '').replace(/\\(.)/g, '$1'),
  );
}

export interface Call {
  args: string[];
  /** Offset of the call within the scanned code. */
  index: number;
}

/** Every call `name(…)` in `code`, with its top-level arguments. */
export function findCalls(code: string, name: string): Call[] {
  const calls: Call[] = [];
  const pattern = new RegExp(`(?<![\\w.>])${escapeRegExp(name)}\\s*(?:<[^;(]*>)?\\s*\\(`, 'g');
  for (const m of code.matchAll(pattern)) {
    const open = m.index + m[0].length - 1;
    const close = matchBracket(code, open);
    if (close < 0) continue;
    calls.push({ args: splitTopLevel(code.slice(open + 1, close)), index: m.index });
  }
  return calls;
}

export interface IfBlock {
  condition: string;
  /** The statement or `{…}` block the condition guards, up to the next top-level `else`. */
  body: string;
}

/**
 * Top-level `if (cond) body` statements of a function body, including each `else if` link.
 * Good enough for the DLL's dispatch functions, which are flat if-chains.
 */
export function ifBlocks(code: string): IfBlock[] {
  const blocks: IfBlock[] = [];
  let depth = 0;
  for (let i = 0; i < code.length; i++) {
    const c = code[i];
    if (c === '"' || c === "'") {
      i = literalEnd(code, i) - 1;
      continue;
    }
    if (c === '{') depth++;
    else if (c === '}') depth--;
    if (depth !== 0 || !/^if\s*\(/.test(code.slice(i, i + 4)) || /\w/.test(code[i - 1] ?? '')) {
      continue;
    }
    const open = code.indexOf('(', i);
    const close = matchBracket(code, open);
    if (close < 0) break;
    const bodyStart = skipSpace(code, close + 1);
    let bodyEnd: number;
    if (code[bodyStart] === '{') {
      bodyEnd = matchBracket(code, bodyStart) + 1;
    } else {
      bodyEnd = statementEnd(code, bodyStart);
    }
    blocks.push({ condition: code.slice(open + 1, close), body: code.slice(bodyStart, bodyEnd) });
    i = bodyEnd - 1;
  }
  return blocks;
}

/** Index after the `;` ending the statement starting at `start`. */
function statementEnd(code: string, start: number): number {
  let depth = 0;
  for (let i = start; i < code.length; i++) {
    const c = code[i];
    if (c === '"' || c === "'") {
      i = literalEnd(code, i) - 1;
    } else if (c === '(' || c === '{' || c === '[') {
      depth++;
    } else if (c === ')' || c === '}' || c === ']') {
      depth--;
    } else if (c === ';' && depth === 0) {
      return i + 1;
    }
  }
  return code.length;
}
