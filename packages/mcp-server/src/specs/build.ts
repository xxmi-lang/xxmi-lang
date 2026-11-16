/**
 * Builds the final spec of one DLL commit: the extractor's output plus the overlay shipped with
 * this server build, the same merge core does for the bundled spec.
 */
import { createHash } from 'node:crypto';
import {
  applyOverlays,
  defaultOverlays,
  VERSION,
  type Spec,
  type SpecOverlayFile,
} from '@xxmi-lang/core';
import { EXTRACTOR_VERSION, extractSpec, type GitInfo } from '@xxmi-lang/spec-extractor';

/** One extractor input: a materialized checkout and the commit it came from. */
export interface SourceInput {
  dir: string;
  commit: string;
  commitDate?: string;
  repository?: string;
}

export interface BuildInput {
  dll: SourceInput;
  baseline: SourceInput;
}

export interface BuiltSpec {
  spec: Spec;
  warnings: string[];
}

/** Materialized sources are not git checkouts, so the extractor gets the commit info explicitly. */
const gitInfoOf = (source: SourceInput): GitInfo => ({
  repository: source.repository ?? '',
  commit: source.commit,
  commitDate: source.commitDate ?? '',
});

export function buildVersionSpec(
  input: BuildInput,
  overlays: readonly SpecOverlayFile[] = defaultOverlays(),
): BuiltSpec {
  const { spec, baselineWarnings } = extractSpec({
    dllRoot: input.dll.dir,
    baselineRoot: input.baseline.dir,
    dllGit: gitInfoOf(input.dll),
    baselineGit: gitInfoOf(input.baseline),
  });

  // Entries an older DLL lacks are skipped with a problem instead of failing the build.
  const problems = applyOverlays(spec, overlays);
  return {
    spec,
    warnings: [
      ...baselineWarnings.map((w) => `baseline: ${w}`),
      ...problems.map((p) => `overlay: ${p}`),
    ],
  };
}

/**
 * Names everything a stored spec depends on besides the sources: a deploy that changes the
 * release, the extractor or the overlay rebuilds the specs.
 */
export function builderId(overlays: readonly SpecOverlayFile[] = defaultOverlays()): string {
  const overlayHash = createHash('sha1').update(JSON.stringify(overlays)).digest('hex').slice(0, 8);
  return `${VERSION}+extractor${EXTRACTOR_VERSION}+overlay${overlayHash}`;
}
