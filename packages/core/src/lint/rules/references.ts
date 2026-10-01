/**
 * Reference and namespace rules (XM2xx). Resolution follows the DLL's lookup order; see
 * workspace/symbols.ts.
 */
import { dirname } from 'node:path';
import type { IniFile, Reference, SymbolKind } from '../../model/types.ts';
import { didYouMean, replaceFix } from '../suggest.ts';
import { explicitNamespace, globalVariableKey, type SymbolTable } from '../../workspace/symbols.ts';
import type { Workspace } from '../../workspace/workspace.ts';
import type { Rule, RuleContext } from '../types.ts';

interface Usage {
  /** Lower-cased qualified names of sections some reference resolved to. */
  sections: Set<string>;
  /** Keys of global variables some reference resolved to. */
  globals: Set<string>;
  /** Local declarations (by `path#offset`) some reference resolved to. */
  locals: Set<string>;
}

const usageCache = new WeakMap<SymbolTable, Usage>();

/** Resolves every reference in the workspace once, recording what is used. */
function usage(workspace: Workspace): Usage {
  const symbols = workspace.symbols;
  let result = usageCache.get(symbols);
  if (result) return result;
  result = { sections: new Set(), globals: new Set(), locals: new Set() };
  for (const file of workspace.files.values()) {
    for (const ref of file.references) {
      if (ref.kind === 'variable') {
        const found = symbols.resolveVariable(ref, file);
        if (!found) continue;
        if (found.declaration?.scope === 'local')
          result.locals.add(`${file.path}#${found.declaration.span.start}`);
        else result.globals.add(found.key);
      } else {
        const found = symbols.resolveSection(ref.kind, ref.text, file.namespace);
        if (found) result.sections.add(found.qualifiedName.toLowerCase());
      }
    }
  }
  usageCache.set(symbols, result);
  return result;
}

/** Names of symbols of `kind` reachable from `file` without a namespace, for suggestions. */
function localNames(symbols: SymbolTable, kind: SymbolKind, file: IniFile): string[] {
  const own = file.namespace.toLowerCase();
  return symbols
    .allSections()
    .filter((s) => s.kind === kind)
    .map((s) => {
      const q = s.qualifiedName;
      const inOwn = own !== '' && q.toLowerCase().includes(`\\${own}\\`);
      return inOwn ? q.replace(/\\.*\\/, '') : q;
    });
}

function checkSectionRefs(context: RuleContext, kinds: SymbolKind[]): void {
  const { file, symbols, lookup, report } = context;
  for (const ref of file.references) {
    if (!kinds.includes(ref.kind)) continue;
    if (symbols.resolveSection(ref.kind, ref.text, file.namespace)) continue;
    const ns = explicitNamespace(ref.text, lookup);
    if (ns !== undefined && !symbols.hasNamespace(ns)) continue; // XM206 reports it
    const names = localNames(symbols, ref.kind, file);
    report(
      ref.span.range,
      `Unresolved ${describe(ref)} \`${ref.text}\`.${didYouMean(ref.text, names)}`,
      replaceFix(ref.span.range, ref.text, names),
    );
  }
}

function describe(ref: Reference): string {
  switch (ref.kind) {
    case 'commandlist':
      return 'command list';
    case 'customshader':
      return 'custom shader';
    case 'pool':
      return 'resource pool';
    default:
      return ref.kind;
  }
}

export const XM201: Rule = {
  id: 'XM201',
  severity: 'error',
  description: 'Unresolved `run = CommandList…` / `CustomShader…` target',
  check(context) {
    checkSectionRefs(context, ['commandlist', 'customshader']);
  },
};

export const XM202: Rule = {
  id: 'XM202',
  severity: 'error',
  description: 'Unresolved resource reference (`ref`, `copy`, `Resource\\N\\X`, pools)',
  check(context) {
    checkSectionRefs(context, ['resource', 'pool']);
  },
};

export const XM203: Rule = {
  id: 'XM203',
  severity: 'warning',
  description: 'Variable used but never declared',
  check({ file, symbols, lookup, report }) {
    for (const ref of file.references) {
      if (ref.kind !== 'variable' || symbols.resolveVariable(ref, file)) continue;
      const ns = explicitNamespace(ref.text, lookup);
      if (ns !== undefined && !symbols.hasNamespace(ns)) continue; // XM206 reports it
      report(
        ref.span.range,
        `Unknown variable: ${ref.text} (declare it with \`global\` in [Constants] or \`local\`)`,
      );
    }
  },
};

export const XM204: Rule = {
  id: 'XM204',
  severity: 'warning',
  description: 'Variable declared but never used',
  check({ file, workspace, report }) {
    // Globals in files pulled in by includes are usually a library's API for other mods.
    const checkGlobals = workspace.targets.has(file.path) && !isLibrary(file, workspace);
    const used = usage(workspace);
    for (const v of file.variables) {
      const isUsed =
        v.scope === 'local'
          ? used.locals.has(`${file.path}#${v.span.start}`)
          : !checkGlobals || used.globals.has(globalVariableKey(v.name, file.namespace));
      if (!isUsed)
        report(
          v.span.range,
          `${v.scope === 'local' ? 'Local' : 'Global'} variable ${v.name} is never used`,
        );
    }
  },
};

/** Files under a package's `Core/` folder are libraries other mods call into. */
function isLibrary(file: IniFile, workspace: Workspace): boolean {
  const root = workspace.packageRoot(file);
  if (!root) return false;
  const rel = file.path.slice(root.length + 1).split(/[\\/]/)[0];
  return rel?.toLowerCase() === 'core';
}

const REFERENCED_KINDS = new Set<SymbolKind>(['commandlist', 'resource', 'customshader']);

export const XM205: Rule = {
  id: 'XM205',
  severity: 'info',
  description: 'Section defined but never referenced (CommandList, Resource, CustomShader)',
  check({ file, workspace, report }) {
    // Library files and d3dx.ini (no namespace: its sections are global) are called from mods.
    if (!workspace.targets.has(file.path) || isLibrary(file, workspace) || file.namespace === '')
      return;
    const used = usage(workspace).sections;
    for (const section of file.sections) {
      if (!section.kind || !REFERENCED_KINDS.has(section.kind)) continue;
      if (!used.has(section.qualifiedName.toLowerCase())) {
        report(section.name.span.range, `[${section.name.text}] is never referenced`);
      }
    }
  },
};

export const XM206: Rule = {
  id: 'XM206',
  severity: 'error',
  description: 'Unresolved namespace in `\\N\\name`',
  check({ file, symbols, lookup, report }) {
    for (const ref of file.references) {
      const ns = explicitNamespace(ref.text, lookup);
      if (ns === undefined || symbols.hasNamespace(ns)) continue;
      report(
        ref.span.range,
        `Unknown namespace \`${ns}\` in \`${ref.text}\`: no loaded file declares \`namespace = ${ns}\``,
      );
    }
  },
};

export const XM207: Rule = {
  id: 'XM207',
  severity: 'error',
  description: 'Include target file not found (`include =`)',
  check({ file, resolver, report }) {
    for (const section of file.sections) {
      if (section.spec?.name !== 'Include') continue;
      for (const line of section.lines) {
        // Only `include`: ParseNamespacedIniFile warns "Error opening" for a missing file, while
        // a missing `include_recursive` folder is only logged at info level (ParseIniFilesRecursive).
        if (line.key?.text.toLowerCase() !== 'include' || !line.value?.text) continue;
        const found = resolver.resolve(dirname(file.path), line.value.text);
        if (!found || resolver.isDirectory(found.path)) {
          report(
            line.value.span.range,
            `Error opening include file: ${line.value.text} (not found)`,
          );
        }
      }
    }
  },
};

export const XM208: Rule = {
  id: 'XM208',
  severity: 'warning',
  description: '`filename =` points to a missing file',
  check({ file, resolver, packageRoot, report }) {
    for (const section of file.sections) {
      const name = section.spec?.name;
      if (name !== 'Resource' && name !== 'Pool') continue;
      for (const line of section.lines) {
        if (line.key?.text.toLowerCase() !== 'filename' || !line.value?.text) continue;
        if (/^[a-z]:[\\/]/i.test(line.value.text)) continue; // absolute Windows path: can't check here
        // ParseResourceSection: relative to the declaring ini's folder, then the 3DMigoto folder.
        const found =
          resolver.resolve(dirname(file.path), line.value.text) ??
          (packageRoot ? resolver.resolve(packageRoot, line.value.text) : undefined);
        if (!found || resolver.isDirectory(found.path)) {
          report(line.value.span.range, `File not found: ${line.value.text}`);
        }
      }
    }
  },
};
