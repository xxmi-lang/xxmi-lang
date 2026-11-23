/** Grades an XM107 duplicate whose first copy is in another file by what ignoring it costs. */
import { isMeshSlot } from '../../model/mesh.ts';
import type { IniFile, SectionModel } from '../../model/types.ts';
import type { Workspace } from '../../workspace/workspace.ts';
import type { Severity } from '../types.ts';
import { usage } from './references.ts';

export interface Grade {
  severity: Severity;
  note?: string;
}

interface Copy {
  file: IniFile;
  section: SectionModel;
}

export function gradeDuplicate(workspace: Workspace, duplicate: Copy, winner: Copy): Grade {
  if (sameLines(duplicate, winner)) return { severity: 'info', note: 'identical to the first' };
  if (isMeshResource(workspace, duplicate))
    return { severity: 'error', note: 'ignored mesh resource' };
  if (isMeshList(duplicate)) return { severity: 'error', note: 'ignored mesh command list' };
  return { severity: 'warning' };
}

/** Key case and whitespace don't matter to the DLL; comment lines never reach the model. */
function normalizedLines({ section }: Copy): string[] {
  const collapse = (text: string): string => text.split(/\s+/).filter(Boolean).join(' ');
  return section.lines.map((line) =>
    line.key && line.value
      ? `${collapse(line.key.text).toLowerCase()}=${collapse(line.value.text)}`
      : collapse(line.content.text),
  );
}

function sameLines(a: Copy, b: Copy): boolean {
  const left = normalizedLines(a);
  const right = normalizedLines(b);
  return left.length === right.length && left.every((line, i) => line === right[i]);
}

/** Values of the section's lines whose key (minus any `pre`/`post`) is `command`. */
function valuesOf(section: SectionModel, command: string): string[] {
  return section.lines.filter((l) => l.command === command).map((l) => l.value?.text ?? '');
}

/** A Resource a `vb0`-`vb2` or `ib` line binds anywhere in the workspace, or backed by a `.buf` or `.ib` file. */
function isMeshResource(workspace: Workspace, { section }: Copy): boolean {
  if (section.kind !== 'resource') return false;
  if (usage(workspace).meshBound.has(section.qualifiedName.toLowerCase())) return true;
  return valuesOf(section, 'filename').some((v) => /\.(buf|ib)$/i.test(v));
}

/** A CommandList or TextureOverride that binds any `vb*` or `ib`, or a TextureOverride that skips or draws. */
function isMeshList({ section }: Copy): boolean {
  if (section.kind !== 'commandlist' && section.kind !== 'textureoverride') return false;
  if (section.lines.some((l) => l.command !== undefined && isMeshSlot(l.command))) return true;
  return (
    section.kind === 'textureoverride' &&
    valuesOf(section, 'handling').some((v) =>
      ['skip', 'draw', 'drawindexed'].includes(v.toLowerCase()),
    )
  );
}
