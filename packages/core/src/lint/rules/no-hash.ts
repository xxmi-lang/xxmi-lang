import type { SectionModel, TextSpan } from '../../model/types.ts';
import type { Rule } from '../types.ts';

/**
 * TEXTURE_OVERRIDE_FUZZY_MATCHES in IniHandler.cpp: the keys that stand in for `hash`.
 * `filter_index`, `match_priority` and the draw-call `match_first_index`-style keys don't count.
 */
const RESOURCE_MATCH_KEYS = new Set([
  'match_type',
  'match_usage',
  'match_bind_flags',
  'match_cpu_access_flags',
  'match_misc_flags',
  'match_byte_width',
  'match_stride',
  'match_mips',
  'match_format',
  'match_width',
  'match_height',
  'match_depth',
  'match_array',
  'match_msaa',
  'match_msaa_quality',
]);

/**
 * The section's lower-cased keys, with their spans.
 * A key with an empty value is dropped at load ("No value found"), so it isn't a key.
 */
function keys(section: SectionModel): { key: string; span: TextSpan }[] {
  return section.lines.flatMap((line) =>
    line.key && line.value?.text !== ''
      ? [{ key: line.key.text.toLowerCase(), span: line.key.span }]
      : [],
  );
}

export const XM112: Rule = {
  id: 'XM112',
  severity: 'warning',
  description:
    'TextureOverride with no `hash` and no `match_*` key, with both, or ShaderOverride with no `hash`',
  check({ file, report }) {
    for (const section of file.sections) {
      const name = section.name.text;
      const sectionKeys = keys(section);
      const hasHash = sectionKeys.some(({ key }) => key === 'hash');

      if (section.spec?.name === 'ShaderOverride' && !hasHash) {
        // ParseShaderOverrideSections: "Section missing Hash=", with no fuzzy fallback.
        report(
          section.name.span.range,
          `[${name}] has no \`hash\`, so the DLL skips it (a ShaderOverride can't match any other way).`,
        );
      }
      if (section.spec?.name !== 'TextureOverride') continue;

      const matches = sectionKeys.filter(({ key }) => RESOURCE_MATCH_KEYS.has(key));
      if (!hasHash && matches.length === 0) {
        // ParseTextureOverrideSections: "Section missing Hash= or valid match options".
        report(
          section.name.span.range,
          `[${name}] has no \`hash\` and no resource \`match_*\` key (\`match_type\`, \`match_format\`, \`match_width\`, …), so the DLL skips it.`,
        );
      } else if (hasHash) {
        // ParseTextureOverrideSections: "Cannot use hash= and match options together!".
        for (const { key, span } of matches) {
          report(span.range, `\`${key}\` is ignored: with \`hash\`, the DLL matches by hash only.`);
        }
      }
    }
  },
};
