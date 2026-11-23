import { accessSync, constants, statSync } from 'node:fs';
import { delimiter, join } from 'node:path';

/** Finds an executable on PATH; `xxmi-lsp` becomes `xxmi-lsp.exe` on Windows. */
export function findOnPath(
  name: string,
  pathVar: string | undefined = process.env.PATH,
  platform: string = process.platform,
): string | undefined {
  const file = platform === 'win32' ? `${name}.exe` : name;
  for (const dir of (pathVar ?? '').split(delimiter)) {
    if (dir && isExecutableFile(join(dir, file), platform)) return join(dir, file);
  }
  return undefined;
}

function isExecutableFile(path: string, platform: string): boolean {
  try {
    if (!statSync(path).isFile()) return false;
    // Windows has no executable bit; the `.exe` suffix is the check.
    if (platform !== 'win32') accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}
