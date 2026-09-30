/**
 * Workspace-wide symbols and DLL-order name resolution (docs/01-language-model.md §3 rules 1–7).
 *
 * The DLL stores a prefixed section from a namespaced file as `Prefix\namespace\Local` and a
 * global declared in namespace N as `$\n\name` (IniHandler.cpp `get_namespaced_section_name`,
 * `get_namespaced_var_name_lower`). A reference first tries the name qualified with the
 * referencing file's namespace, then the name as written (`FindExplicitCommandListSection`,
 * `parse_command_list_var_name`). All comparisons are case-insensitive.
 */
import type { SpecLookup } from '../spec/lookup.ts';
import type { IniFile, Reference, SymbolKind, VariableDeclaration } from '../model/types.ts';

/** Where a symbol is declared: a loaded file, or an entry of a bundled library snapshot. */
export type SymbolOrigin =
  { file: IniFile; section: number } | { snapshot: string; path: string; line: number };

export interface SectionSymbol {
  kind: SymbolKind;
  /** As the DLL stores it, e.g. `CommandList\ZZMI\SetTextures`. */
  qualifiedName: string;
  origin: SymbolOrigin;
}

export interface VariableSymbol {
  /** As the DLL stores it, lower-cased: `$\ttl\alpha`, or `$name` for d3dx.ini globals. */
  key: string;
  persist: boolean;
  locked: boolean;
  origin: SymbolOrigin;
  declaration?: VariableDeclaration;
}

/** Library symbols serialized by `xxmi index`, bundled with core for mods outside a package. */
export interface SymbolSnapshot {
  version: 1;
  name: string;
  source: { repository: string; commit: string };
  namespaces: string[];
  sections: { kind: SymbolKind; qualifiedName: string; path: string; line: number }[];
  variables: { key: string; persist: boolean; locked: boolean; path: string; line: number }[];
}

export function globalVariableKey(name: string, namespace: string): string {
  const lower = name.toLowerCase();
  return namespace === '' ? lower : `$\\${namespace.toLowerCase()}\\${lower.slice(1)}`;
}

export class SymbolTable {
  private readonly sections = new Map<string, SectionSymbol[]>();
  private readonly variables = new Map<string, VariableSymbol[]>();
  private readonly namespaceSet = new Set<string>();
  private readonly lookup: SpecLookup;

  constructor(lookup: SpecLookup) {
    this.lookup = lookup;
  }

  addFile(file: IniFile): void {
    if (file.namespace !== '') this.namespaceSet.add(file.namespace.toLowerCase());
    file.sections.forEach((section, index) => {
      if (!section.kind) return;
      this.addSection({
        kind: section.kind,
        qualifiedName: section.qualifiedName,
        origin: { file, section: index },
      });
    });
    for (const declaration of file.variables) {
      if (declaration.scope !== 'global') continue;
      this.addVariable({
        key: globalVariableKey(declaration.name, file.namespace),
        persist: declaration.persist,
        locked: declaration.locked,
        origin: { file, section: declaration.section },
        declaration,
      });
    }
  }

  addSnapshot(snapshot: SymbolSnapshot): void {
    for (const ns of snapshot.namespaces) this.namespaceSet.add(ns.toLowerCase());
    for (const s of snapshot.sections) {
      this.addSection({
        kind: s.kind,
        qualifiedName: s.qualifiedName,
        origin: { snapshot: snapshot.name, path: s.path, line: s.line },
      });
    }
    for (const v of snapshot.variables) {
      this.addVariable({
        key: v.key,
        persist: v.persist,
        locked: v.locked,
        origin: { snapshot: snapshot.name, path: v.path, line: v.line },
      });
    }
  }

  /** Namespaces declared by loaded files and snapshots (lower-cased). */
  hasNamespace(namespace: string): boolean {
    return this.namespaceSet.has(namespace.toLowerCase());
  }

  /** All sections declared under a qualified name (more than one means a duplicate). */
  sectionsNamed(qualifiedName: string): SectionSymbol[] {
    return this.sections.get(qualifiedName.toLowerCase()) ?? [];
  }

  allSections(): SectionSymbol[] {
    return [...this.sections.values()].flat();
  }

  allVariables(): VariableSymbol[] {
    return [...this.variables.values()].flat();
  }

  /**
   * Resolves a section reference made from `namespace`. Built-in sections (the DLL's own
   * `BuiltInCommandList…` etc.) resolve to a symbol with a `builtin` snapshot origin.
   */
  resolveSection(kind: SymbolKind, text: string, namespace: string): SectionSymbol | undefined {
    for (const candidate of this.sectionCandidates(text, namespace)) {
      const found = this.sections.get(candidate.toLowerCase())?.find((s) => s.kind === kind);
      if (found) return found;
    }
    if (this.lookup.isBuiltinSection(text)) {
      return {
        kind,
        qualifiedName: text,
        origin: { snapshot: 'builtin', path: 'd3d11.dll', line: 0 },
      };
    }
    return undefined;
  }

  /** Candidate stored names for a section reference, in DLL lookup order. */
  sectionCandidates(text: string, namespace: string): string[] {
    const prefix = this.lookup.sectionPrefix(text);
    const candidates: string[] = [];
    if (prefix && namespace !== '') {
      candidates.push(`${prefix}\\${namespace}\\${text.slice(prefix.length)}`);
    }
    candidates.push(text);
    return candidates;
  }

  /**
   * Resolves `$name` / `$\ns\name` used in `file`. Locals declared earlier in the same section
   * win (the DLL checks the command list's scope first), then globals in DLL order.
   */
  resolveVariable(ref: Reference, file: IniFile): VariableSymbol | undefined {
    const lower = ref.text.toLowerCase();
    const local = file.variables.find(
      (v) =>
        v.scope === 'local' &&
        v.section === ref.section &&
        v.span.start <= ref.span.start &&
        v.name.toLowerCase() === lower,
    );
    if (local) {
      return {
        key: lower,
        persist: false,
        locked: false,
        origin: { file, section: local.section },
        declaration: local,
      };
    }
    const candidates =
      file.namespace === '' ? [lower] : [globalVariableKey(ref.text, file.namespace), lower];
    for (const candidate of candidates) {
      const found = this.variables.get(candidate)?.[0];
      if (found) return found;
    }
    return undefined;
  }

  private addSection(symbol: SectionSymbol): void {
    const key = symbol.qualifiedName.toLowerCase();
    const list = this.sections.get(key);
    if (list) list.push(symbol);
    else this.sections.set(key, [symbol]);
  }

  private addVariable(symbol: VariableSymbol): void {
    const list = this.variables.get(symbol.key);
    if (list) list.push(symbol);
    else this.variables.set(symbol.key, [symbol]);
  }
}

/**
 * The namespace named explicitly in a reference (`CommandList\ZZMI\Foo` → `ZZMI`,
 * `$\SlotFix\Matches\slot` → `SlotFix\Matches`), or undefined when it has none.
 */
export function explicitNamespace(text: string, lookup: SpecLookup): string | undefined {
  let rest: string;
  if (text.startsWith('$')) rest = text.slice(1);
  else {
    const prefix = lookup.sectionPrefix(text);
    if (!prefix) return undefined;
    rest = text.slice(prefix.length);
  }
  if (!rest.startsWith('\\')) return undefined;
  const last = rest.lastIndexOf('\\');
  return last > 0 ? rest.slice(1, last) : undefined;
}
