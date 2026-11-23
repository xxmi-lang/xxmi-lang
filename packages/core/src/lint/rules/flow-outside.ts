import type { LineModel, SectionModel } from '../../model/types.ts';
import type { Rule } from '../types.ts';
import { isMisplacedFlow } from './structure.ts';

export const XM111: Rule = {
  id: 'XM111',
  severity: 'error',
  description: '`if`/`elif`/`else`/`endif` outside a command list',
  check({ file, report }) {
    const placed: [SectionModel | undefined, LineModel][] = [
      ...file.preamble.map((line): [undefined, LineModel] => [undefined, line]),
      ...file.sections.flatMap((section) =>
        section.lines.map((line): [SectionModel, LineModel] => [section, line]),
      ),
    ];

    for (const [section, line] of placed) {
      if (!isMisplacedFlow(section, line)) continue;

      const place = section ? `[${section.name.text}]` : 'the preamble';
      report(
        line.content.span.range,
        `Flow control outside a command list: \`${line.content.text}\` in ${place} is never parsed as \`if\`/\`endif\`.`,
      );
    }
  },
};
