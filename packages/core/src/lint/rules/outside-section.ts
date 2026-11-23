import type { LineModel } from '../../model/types.ts';
import { commentOutFix } from '../suggest.ts';
import type { Rule } from '../types.ts';
import { isMisplacedFlow } from './structure.ts';

/** `ParseIniPreamble` honours only `namespace` and `condition`, case-insensitively. */
export function isHonoured(line: LineModel): boolean {
  const key = line.key?.text.toLowerCase();
  return key === 'namespace' || key === 'condition';
}

export const XM003: Rule = {
  id: 'XM003',
  severity: 'warning',
  description: 'Preamble line other than `namespace` / `condition` (the DLL ignores it)',
  check({ file, report }) {
    for (const line of file.preamble) {
      // XM111 owns flow keywords, wherever they stray.
      if (isHonoured(line) || isMisplacedFlow(undefined, line)) continue;

      const range = line.content.span.range;
      report(
        range,
        `Entry outside of section: ${line.content.text}. Only \`namespace\` and \`condition\` are read before the first section; the DLL warns and ignores the line.`,
        commentOutFix(range),
      );
    }
  },
};
