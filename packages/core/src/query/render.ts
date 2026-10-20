/** Compact text for query results, shared by `xxmi query` and the MCP tools. */
import type { SymbolInfo } from './library.ts';
import type { SpecEntry } from './spec.ts';

const indent = (text: string, spaces = 2): string =>
  text
    .split('\n')
    .map((l) => `${' '.repeat(spaces)}${l}`)
    .join('\n');

export interface RenderedLocation {
  path: string;
  line: number;
  /** A browsable URL for the location. */
  link?: string | undefined;
}

export function renderSpecEntry(entry: SpecEntry, dll?: RenderedLocation): string {
  const section = entry.section ? ` (in [${entry.section}])` : '';
  const lines = [
    `${entry.kind} ${entry.name}${section}${entry.xxmi ? '  [XXMI-only]' : ''}`,
    `  spec source: ${entry.source}`,
  ];
  if (dll) lines.push(`  DLL: ${dll.path}:${dll.line}${dll.link ? `  ${dll.link}` : ''}`);
  if (entry.facts.length > 0) lines.push(`  ${entry.facts.join('; ')}`);
  if (entry.values?.length) lines.push(`  values: ${entry.values.join(', ')}`);
  if (entry.since) lines.push(`  since: ${entry.since}`);
  if (entry.deprecated) lines.push(`  deprecated: ${entry.deprecated}`);
  if (entry.doc) lines.push(indent(entry.doc.trim()));
  if (entry.example) lines.push('  example:', indent(entry.example.trim(), 4));
  return lines.join('\n');
}

export function renderSymbol(
  info: SymbolInfo,
  options: { link?: string; note?: string } = {},
): string {
  const lines = [
    `${info.kind} ${info.qualifiedName}`,
    `declared: ${info.location.path}:${info.location.line}${options.link ? `  ${options.link}` : ''}`,
  ];
  if (options.note) lines.push(options.note);
  if (info.doc) {
    lines.push('doc:', indent(info.doc.text));
    const tags = info.doc.tags;
    for (const p of tags?.in ?? []) lines.push(`input: ${p.name} ${p.description}`.trimEnd());
    for (const p of tags?.out ?? []) lines.push(`output: ${p.name} ${p.description}`.trimEnd());
    if (tags?.since) lines.push(`since: ${tags.since}`);
    if (tags?.example) lines.push('example:', indent(tags.example));
  }
  if (info.moduleDoc) {
    lines.push(
      `file header (${info.moduleDoc.path}:${info.moduleDoc.line}):`,
      indent(info.moduleDoc.text),
    );
  }
  if (info.usage.length > 0) {
    lines.push('mentioned in comments (usage examples; inputs are usually listed here):');
    for (const u of info.usage) lines.push(`  ${u.path}:${u.line}`, indent(u.text, 4));
  }
  if (info.references.length > 0) {
    lines.push('references:');
    for (const r of info.references) lines.push(`  ${r.access} ${r.kind} ${r.text}`);
  }
  if (info.namespaceMembers.length > 0) {
    lines.push('same namespace (candidate inputs/outputs):');
    for (const m of info.namespaceMembers)
      lines.push(`  ${m.kind} ${m.name}  (${m.path}:${m.line})`);
  }
  if (info.source) lines.push('source:', indent(info.source));
  return lines.join('\n');
}
