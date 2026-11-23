/** Which command-list keys bind mesh data (port in rust/crates/xxmi-core/src/model/mesh.rs). */
import type { IniFile, Reference } from './types.ts';

/** `vb0`, `vb1`, ... as the DLL dispatches on them (lower-cased key). */
function vertexBufferSlot(command: string): number | undefined {
  const digits = /^vb(\d+)$/.exec(command)?.[1];
  return digits === undefined ? undefined : Number(digits);
}

/** A key that binds any vertex buffer slot or the index buffer. */
export function isMeshSlot(command: string): boolean {
  return command === 'ib' || vertexBufferSlot(command) !== undefined;
}

/** A key that binds the slots a mesh needs to draw: `vb0` to `vb2` and `ib`. */
export function isCoreMeshSlot(command: string): boolean {
  return command === 'ib' || (vertexBufferSlot(command) ?? Infinity) <= 2;
}

/** Whether `reference` is written on a line that binds a core mesh slot (`vb0 = ref ResourceX`). */
export function bindsCoreMeshSlot(file: IniFile, reference: Reference): boolean {
  const section = file.sections[reference.section];
  const line = section?.lines.find(
    (l) => l.content.span.start <= reference.span.start && reference.span.end <= l.content.span.end,
  );
  return line?.command !== undefined && isCoreMeshSlot(line.command);
}
