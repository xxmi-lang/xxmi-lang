import { commentOutFix } from '../suggest.ts';
import type { Rule } from '../types.ts';

export const XM110: Rule = {
  id: 'XM110',
  severity: 'warning',
  description: 'Key with no value (the DLL ignores the line)',
  check({ file, report }) {
    for (const section of file.sections) {
      for (const line of section.lines) {
        if (!line.key || line.value?.text !== '') continue;

        // ParseIniKeyValLine warns "No value found" and drops the line, so the key is never set.
        const range = line.content.span.range;
        report(
          range,
          `No value for \`${line.key.text}\`; the DLL warns and ignores the line.`,
          commentOutFix(range),
        );
      }
    }
  },
};
