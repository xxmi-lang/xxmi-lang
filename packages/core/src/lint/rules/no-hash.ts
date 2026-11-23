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

export const XM112: Rule = {
  id: 'XM112',
  severity: 'warning',
  description: 'TextureOverride with no `hash` and no resource-description `match_*` key',
  check({ file, report }) {
    for (const section of file.sections) {
      if (section.spec?.name !== 'TextureOverride') continue;

      const matchesSomething = section.lines.some((line) => {
        const key = line.key?.text.toLowerCase();
        return key !== undefined && (key === 'hash' || RESOURCE_MATCH_KEYS.has(key));
      });
      if (matchesSomething) continue;

      // ParseTextureOverrideSections: "Section missing Hash= or valid match options".
      report(
        section.name.span.range,
        `[${section.name.text}] has no \`hash\` and no resource \`match_*\` key (\`match_type\`, \`match_format\`, \`match_width\`, …), so the DLL skips it.`,
      );
    }
  },
};
