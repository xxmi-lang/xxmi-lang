import type { Range } from '../../parser/index.ts';
import type { Rule } from '../types.ts';
import { flowKeyword } from './structure.ts';

export const XM403: Rule = {
  id: 'XM403',
  severity: 'hint',
  description: "Flow keyword (`if`, `elif`, `else`, `endif`) that isn't lowercase",
  check({ file, report }) {
    for (const section of file.sections) {
      if (section.spec?.kind !== 'commandlist') continue;

      for (const line of section.lines) {
        const keyword = flowKeyword(line);
        if (!keyword) continue;

        // `else if` is one keyword for the DLL's `elif` handling.
        const length =
          keyword === 'elif' && /^else if/i.test(line.content.text) ? 7 : keyword.length;
        const written = line.content.text.slice(0, length);
        const lower = written.toLowerCase();
        if (written === lower) continue;

        const { start } = line.content.span.range;
        const range: Range = { start, end: { ...start, character: start.character + length } };
        // ParseCommandList lower-cases the whole line before ParseCommandListFlowControl.
        report(
          range,
          `Flow keyword \`${written}\` should be lowercase; the DLL accepts any case.`,
          {
            title: `Change to \`${lower}\``,
            edits: [{ range, newText: lower }],
          },
        );
      }
    }
  },
};
