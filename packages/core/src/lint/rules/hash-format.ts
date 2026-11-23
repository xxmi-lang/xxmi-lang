import type { Rule, Severity } from '../types.ts';
import { commentStart } from './inline-comment.ts';

/** `%16llx` in GetIniHash counts the `0x` prefix in its width. */
const MAX_CHARS = 16;

/** `%llx` follows the C standard and takes an optional sign; a `-` negates the unsigned value. */
function signedMessage(written: string, sign: string, texture: boolean): string {
  const wraps = sign === '-' ? ' (`-` wraps the value to `ffffffff…`)' : '';
  const expected = texture ? '8' : '8 or 16';
  return `Signed hash "${written}": the DLL accepts the sign${wraps}, but hashes are written as ${expected} hex digits with no sign or \`0x\` prefix.`;
}

/**
 * GetIniHash reads `sscanf("%16llx%n")` and rejects anything left over,
 * so `0x` and short forms parse.
 * A TextureOverride casts the result to 32 bits, silently dropping the high digits.
 */
export function classifyHash(
  written: string,
  texture: boolean,
): { severity: Severity; message: string } | undefined {
  const sign = /^[+-]/.exec(written)?.[0];
  const unsigned = sign ? written.slice(1) : written;
  const prefixed = /^0x/i.test(unsigned);
  const digits = prefixed ? unsigned.slice(2) : unsigned;

  const readable = /^[0-9a-f]+$/i.test(digits) && written.length <= MAX_CHARS;
  if (!readable) {
    return {
      severity: 'error',
      message: `Invalid hash "${written}": the DLL rejects it (Hash parse error) and treats the key as missing.`,
    };
  }

  if (texture && digits.length > 8) {
    return {
      severity: 'error',
      message: `Hash "${written}" has more than 8 digits: a TextureOverride hash is 32-bit, so the DLL drops the high digits.`,
    };
  }

  if (sign) return { severity: 'warning', message: signedMessage(written, sign, texture) };

  const canonicalLength = digits.length === 8 || (!texture && digits.length === 16);
  if (!prefixed && canonicalLength) return undefined;
  const expected = texture ? '8' : '8 or 16';
  return {
    severity: 'warning',
    message: `Non-canonical hash "${written}": the DLL accepts it, but hashes are written as ${expected} hex digits with no \`0x\` prefix.`,
  };
}

export const XM113: Rule = {
  id: 'XM113',
  severity: 'error',
  description:
    "Hash value the DLL can't read as hex, or that isn't canonical (error or warning per finding)",
  check({ file, lookup, report }) {
    for (const section of file.sections) {
      const spec = section.spec;
      if (!spec) continue;
      const texture = spec.name === 'TextureOverride';

      for (const line of section.lines) {
        if (!line.key || !line.value) continue;
        const written = line.value.text;
        // XM110 and XM115 own empty values and values with a trailing `;` comment.
        if (written === '' || commentStart(written) !== undefined) continue;
        if (lookup.key(spec, line.key.text)?.valueType !== 'hash') continue;

        const finding = classifyHash(written, texture);
        if (finding) {
          report(line.value.span.range, finding.message, undefined, finding.severity);
        }
      }
    }
  },
};
