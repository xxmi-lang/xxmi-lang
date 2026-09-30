/**
 * Fast, DLL-faithful lookups over a loaded spec: section-kind matching follows `SectionPrefix` /
 * `SectionInList` (IniHandler.cpp), command matching follows the `wcscmp`/`wcsncmp` dispatch.
 */
import type { SpecCommand, SpecEnum, SpecKey, SpecSection, Spec } from './types.ts';
import type { SymbolKind } from '../model/types.ts';

/** Symbol kind declared by sections of each spec section kind (by DLL table name). */
const SECTION_SYMBOL: Record<string, SymbolKind> = {
  commandlist: 'commandlist',
  builtincommandlist: 'commandlist',
  customshader: 'customshader',
  builtincustomshader: 'customshader',
  resource: 'resource',
  pool: 'pool',
  preset: 'preset',
  key: 'key',
  textureoverride: 'textureoverride',
  shaderoverride: 'shaderoverride',
  shaderregex: 'shaderregex',
};

export class SpecLookup {
  readonly spec: Spec;
  private readonly commandListSections: SpecSection[];
  private readonly regularSections: SpecSection[];
  private readonly exactCommands = new Map<string, SpecCommand>();
  private readonly prefixCommands: SpecCommand[] = [];
  private readonly enums = new Map<string, SpecEnum>();
  private readonly builtins = new Set<string>();

  constructor(spec: Spec) {
    this.spec = spec;
    this.commandListSections = spec.sections.filter((s) => s.kind === 'commandlist');
    this.regularSections = spec.sections.filter((s) => s.kind === 'regular');
    for (const c of spec.commands) {
      if (c.kind === 'flow' || c.kind === 'prefix') continue;
      if (c.match === 'exact') this.exactCommands.set(c.name.toLowerCase(), c);
      else this.prefixCommands.push(c);
    }
    for (const e of spec.enums) this.enums.set(e.name, e);
    for (const b of spec.builtinSections) this.builtins.add(b.name.toLowerCase());
  }

  /** The section kind a `[name]` belongs to (`IsCommandListSection`, then `IsRegularSection`). */
  sectionKind(name: string): SpecSection | undefined {
    const lower = name.toLowerCase();
    const matches = (s: SpecSection): boolean =>
      s.prefix ? lower.startsWith(s.name.toLowerCase()) : lower === s.name.toLowerCase();
    return this.commandListSections.find(matches) ?? this.regularSections.find(matches);
  }

  /**
   * The namespacing prefix of `name` (`SectionPrefix`): only prefix entries count, command-list
   * table first. Returns the prefix as spelled in the DLL table.
   */
  sectionPrefix(name: string): string | undefined {
    const lower = name.toLowerCase();
    const matches = (s: SpecSection): boolean => s.prefix && lower.startsWith(s.name.toLowerCase());
    return (this.commandListSections.find(matches) ?? this.regularSections.find(matches))?.name;
  }

  symbolKind(section: SpecSection): SymbolKind | undefined {
    return SECTION_SYMBOL[section.name.toLowerCase()];
  }

  /** The command a command-list key dispatches to, exact names first, then prefixes. */
  command(key: string): SpecCommand | undefined {
    const lower = key.toLowerCase();
    return (
      this.exactCommands.get(lower) ??
      this.prefixCommands.find((c) => lower.startsWith(c.name.toLowerCase()))
    );
  }

  key(section: SpecSection, key: string): SpecKey | undefined {
    const lower = key.toLowerCase();
    return section.keys.find((k) => k.name.toLowerCase() === lower);
  }

  enumTable(name: string): SpecEnum | undefined {
    return this.enums.get(name);
  }

  isBuiltinSection(name: string): boolean {
    return this.builtins.has(name.toLowerCase());
  }
}
