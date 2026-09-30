/**
 * Shape of spec/generated/*.json (written by @xxmi-lang/spec-extractor) and of the merged spec
 * after spec/overlay/*.toml is applied. See docs/01-language-model.md §1.
 */

/** Present on every generated entry, e.g. `"IniHandler.cpp:ParseResourceSection"`. */
export interface Sourced {
  /**
   * `<file>:<symbol>` in the XXMI DLL source, or `"manual"` for hand-written overlay entries.
   */
  source: string;
  /** In the XXMI fork but not in vanilla 3DMigoto. */
  xxmi: boolean;
}

/** Documentation fields only overlays provide. */
export interface Documented {
  doc?: string;
  example?: string;
  since?: string;
  deprecated?: string;
  /** Required on `source = "manual"` overlay entries: why the entry exists. */
  note?: string;
}

export type SectionKind = 'commandlist' | 'regular';

export interface SpecKey extends Sourced, Documented {
  name: string;
  /**
   * `int`, `float`, `bool`, `string`, `hex`, `bool-or-int`, `hash`, `key-binding`,
   * `enum:<EnumTableName>`, or `unknown` when the DLL reads the key without a typed getter.
   */
  valueType: string;
  /** The DLL reads every occurrence of the key (e.g. `key` and `back` in `[Key*]`). */
  repeatable?: boolean;
}

export interface SpecDynamicKeys extends Sourced {
  /** `variable`: `$name = value` lines; `ini_param`: `x`, `y1`, `w12`, … */
  kind: 'variable' | 'ini_param';
}

export interface SpecSection extends Sourced, Documented {
  name: string;
  /** Matches any section whose name starts with `name` (case-insensitive). */
  prefix: boolean;
  kind: SectionKind;
  /** Lines without `=` are allowed (flow control in command lists, ShaderRegex patterns, …). */
  allowsBareLines: boolean;
  /** Repeated keys don't trigger the DLL's duplicate-key warning. */
  allowsDuplicateKeys: boolean;
  /** Keys read by the section parser. Command-list sections also accept every command. */
  keys: SpecKey[];
  /**
   * Keys vanilla 3DMigoto reads in this section that XXMI no longer does (stereo options, …).
   * Old d3dx.ini files still carry them; the DLL ignores them. `source` is the vanilla location.
   */
  removedKeys: { name: string; source: string }[];
  dynamicKeys: SpecDynamicKeys[];
}

/** A section the DLL defines itself (`InsertBuiltInIniSections`), e.g. `BuiltInCommandListUnbindAllRenderTargets`. */
export interface SpecBuiltinSection extends Sourced {
  name: string;
}

export interface SpecSections {
  sections: SpecSection[];
  builtinSections: SpecBuiltinSection[];
}

export type CommandKind = 'general' | 'draw' | 'flow' | 'declaration' | 'prefix';

export interface SpecCommandValue extends Sourced {
  name: string;
  match: 'exact' | 'prefix';
}

export interface SpecCommand extends Sourced, Documented {
  name: string;
  kind: CommandKind;
  /** `prefix`: the DLL matches the start of the key (e.g. `commandlist…`) or line (`if `). */
  match: 'exact' | 'prefix';
  /** Keyword values the DLL recognises specially (`handling = skip`, `draw = auto`, …). */
  values: SpecCommandValue[];
  /**
   * Number of comma-separated arguments: from `ParseDrawCommandArgs` for draw commands, or from
   * the separators a `CommandArgumentReader`-based parser consumes (e.g. `store`).
   */
  argCount?: number;
  /** First argument is a resource holding the indirect arguments. */
  indirect?: boolean;
}

export interface SpecResourceMember extends Sourced, Documented {
  /** Without the `->`, e.g. `Region`. Matching is case-insensitive. */
  name: string;
  /** Argument kinds from the DLL's member table: `unsigned`, `float`, `string`. */
  args: string[];
}

export interface SpecFunction extends Sourced, Documented {
  name: string;
}

export interface SpecCommands {
  commands: SpecCommand[];
  resourceMembers: SpecResourceMember[];
  functions: SpecFunction[];
}

export interface SpecOperatorToken extends Sourced, Documented {
  token: string;
}

export interface SpecPrecedenceLevel extends Sourced {
  /** 0 binds tightest. */
  level: number;
  /** DLL array name, e.g. `shift_operators`. */
  group: string;
  associativity: 'left' | 'right';
  unary: boolean;
  operators: string[];
}

export interface SpecOperators {
  tokens: SpecOperatorToken[];
  precedence: SpecPrecedenceLevel[];
}

export interface SpecEnumValue extends Sourced {
  name: string;
}

export interface SpecEnum extends Sourced, Documented {
  /** DLL table name, e.g. `PoolIndexTypeNames`; referenced as `enum:<name>`. */
  name: string;
  values: SpecEnumValue[];
}

export interface SpecEnums {
  enums: SpecEnum[];
}

export interface SpecMeta {
  extractorVersion: number;
  dll: { repository: string; commit: string; commitDate: string };
  /** Vanilla 3DMigoto checkout used to decide `xxmi` flags. */
  baseline: { repository: string; commit: string; commitDate: string };
}

export interface Spec {
  meta: SpecMeta;
  sections: SpecSection[];
  builtinSections: SpecBuiltinSection[];
  commands: SpecCommand[];
  resourceMembers: SpecResourceMember[];
  functions: SpecFunction[];
  operators: SpecOperators;
  enums: SpecEnum[];
}
