/** Library entry: the CLI lives in cli.ts and runs on import, so it is not exported here. */
export { AnchorNotFoundError } from './cpp.ts';
export type { GitInfo } from './dll.ts';
export {
  EXTRACTOR_VERSION,
  extractSpec,
  serializeSpec,
  type ExtractOptions,
  type ExtractResult,
} from './extract.ts';
