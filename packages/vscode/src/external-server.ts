import { findOnPath } from './which.ts';

export interface ExternalServer {
  command: string;
  args: string[];
}

/** The user-chosen or PATH server; undefined means "use the bundled one". */
export function externalServer(
  custom: string | undefined,
  preferPath: boolean,
  pathVar?: string,
  platform?: string,
): ExternalServer | undefined {
  // `--stdio` alone is understood by both `xxmi` and a standalone `xxmi-lsp`.
  if (custom) return { command: custom, args: ['--stdio'] };

  const onPath = preferPath ? findOnPath('xxmi-lsp', pathVar, platform) : undefined;
  return onPath ? { command: onPath, args: ['--stdio'] } : undefined;
}
