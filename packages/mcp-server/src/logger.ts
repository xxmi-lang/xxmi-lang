/**
 * Structured logs, one JSON object per line on stdout (Railway indexes them as fields).
 * Callers pass sizes, timings and counts only: never the text submitted to `lint_ini`.
 */
export type LogFields = Record<string, unknown>;

export interface Logger {
  info(msg: string, fields?: LogFields): void;
  warn(msg: string, fields?: LogFields): void;
  error(msg: string, fields?: LogFields): void;
}

export function createLogger(
  write: (line: string) => void = (l) => process.stdout.write(l),
): Logger {
  const emit =
    (level: string) =>
    (msg: string, fields: LogFields = {}): void => {
      write(`${JSON.stringify({ ts: new Date().toISOString(), level, msg, ...fields })}\n`);
    };
  return { info: emit('info'), warn: emit('warn'), error: emit('error') };
}

export const silentLogger: Logger = { info() {}, warn() {}, error() {} };

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
