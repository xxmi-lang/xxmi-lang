/**
 * `xxmi.toml`: the nearest one up the tree from a file wins (docs/02-diagnostics.md
 * "Configuration").
 *
 *   [rules]
 *   XM204 = "off"
 *   XM401 = "warning"
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { parse as parseToml } from 'smol-toml';
import type { Severity } from './types.ts';

export type RuleSetting = Severity | 'off';

export interface LintConfig {
  /** Host path of the xxmi.toml, if one was found. */
  path?: string;
  rules: Record<string, RuleSetting>;
}

const SETTINGS = new Set<RuleSetting>(['off', 'error', 'warning', 'info', 'hint']);

export class ConfigError extends Error {
  constructor(path: string, message: string) {
    super(`${path}: ${message}`);
    this.name = 'ConfigError';
  }
}

export function parseConfig(text: string, path: string): LintConfig {
  let data: unknown;
  try {
    data = parseToml(text);
  } catch (error) {
    throw new ConfigError(path, error instanceof Error ? error.message : String(error));
  }
  const config: LintConfig = { path, rules: {} };
  const rules = (data as { rules?: unknown }).rules;
  if (rules === undefined) return config;
  if (typeof rules !== 'object' || rules === null)
    throw new ConfigError(path, '[rules] must be a table');
  for (const [id, value] of Object.entries(rules)) {
    if (!/^XM\d{3}$/.test(id)) throw new ConfigError(path, `unknown rule id ${id}`);
    // A table ([rules.XM402]) holds rule options; its presence alone doesn't change severity.
    if (typeof value === 'object') continue;
    if (typeof value !== 'string' || !SETTINGS.has(value as RuleSetting)) {
      throw new ConfigError(path, `${id} must be one of ${[...SETTINGS].join(', ')}`);
    }
    config.rules[id] = value as RuleSetting;
  }
  return config;
}

/** Finds and parses the nearest xxmi.toml at or above `dir`, caching per directory. */
export class ConfigLoader {
  private readonly cache = new Map<string, LintConfig>();

  forDirectory(dir: string): LintConfig {
    const cached = this.cache.get(dir);
    if (cached) return cached;
    let config: LintConfig;
    let text: string | undefined;
    try {
      text = readFileSync(join(dir, 'xxmi.toml'), 'utf8');
    } catch {
      text = undefined;
    }
    if (text !== undefined) config = parseConfig(text, join(dir, 'xxmi.toml'));
    else {
      const parent = dirname(dir);
      config = parent === dir ? { rules: {} } : this.forDirectory(parent);
    }
    this.cache.set(dir, config);
    return config;
  }
}
