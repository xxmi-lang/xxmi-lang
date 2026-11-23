import type { IniFile } from '../../model/types.ts';
import { compareLoadOrder, relativeToRoot } from '../../workspace/load-order.ts';
import type { Workspace } from '../../workspace/workspace.ts';

/**
 * The files the DLL loads together with `file`, in workspace order.
 * Files it never loads (`exclude_recursive`, e.g. DISABLED folders) can only clash with themselves:
 * a disabled copy of a library isn't a duplicate of the enabled one.
 * One lint run can span several packages; the DLL only ever loads one.
 */
export function loadedFiles(workspace: Workspace, file: IniFile): IniFile[] {
  const excluded = workspace.isExcluded(file.path);
  const ownPackage = workspace.packageRoot(file);
  return [...workspace.files.values()].filter(
    (other) =>
      (other === file || !(excluded || workspace.isExcluded(other.path))) &&
      workspace.packageRoot(other) === ownPackage,
  );
}

/** Whether the DLL parses file `a` before file `b` (both from `file`'s package). */
export function loadsBefore(workspace: Workspace, file: IniFile, a: IniFile, b: IniFile): boolean {
  const root = workspace.packageRoot(file);
  return compareLoadOrder(relativeToRoot(a.path, root), relativeToRoot(b.path, root)) < 0;
}
