/** Builds one `McpServer` with every tool and resource (one per request in stateless mode). */
import { McpServer, ResourceTemplate } from '@modelcontextprotocol/sdk/server/mcp.js';
import { VERSION, type Spec } from '@xxmi-lang/core';
import { CatalogError } from '../catalog.ts';
import { errorMessage } from '../logger.ts';
import { SourceError } from '../sources.ts';
import type { ToolDefinition } from './define.ts';
import { libraryList, librarySymbol } from './library.ts';
import { lintIni } from './lint.ts';
import { findImplementationTool, readSourceTool, searchSourceTool } from './source.ts';
import { specList, specLookup } from './spec.ts';
import { resolveSpec, specBlock, ToolError, withTimeout, type ToolContext } from './shared.ts';
import { listVersions } from './versions.ts';

export const TOOLS = [
  specLookup,
  specList,
  libraryList,
  librarySymbol,
  findImplementationTool,
  searchSourceTool,
  readSourceTool,
  lintIni,
  listVersions,
] as const;

const INSTRUCTIONS =
  'Read-only lookups for 3DMigoto/XXMI mod ini files: spec_lookup/spec_list for sections, keys and commands; library_list/library_symbol for game-package libraries (e.g. CommandList\\ZZMI\\SetTextures); find_implementation/search_source/read_source for the XXMI DLL and package source; lint_ini to lint ini text.';

function isExpected(error: unknown): boolean {
  return (
    error instanceof ToolError || error instanceof CatalogError || error instanceof SourceError
  );
}

function register(server: McpServer, ctx: ToolContext, def: ToolDefinition): void {
  server.registerTool(
    def.name,
    { description: def.description, inputSchema: def.input },
    async (raw) => {
      const args = raw as Parameters<typeof def.run>[1];
      const started = Date.now();
      try {
        const out = await withTimeout(def.run(ctx, args), ctx.config.toolTimeoutMs, def.name);
        const format = (args as { format?: string }).format;
        ctx.logger.info('tool', { tool: def.name, ms: Date.now() - started, ok: true });
        return {
          content: [
            {
              type: 'text' as const,
              text: format === 'json' ? JSON.stringify(out.data, null, 2) : out.text,
            },
          ],
        };
      } catch (error) {
        const expected = isExpected(error);
        ctx.logger.info('tool', { tool: def.name, ms: Date.now() - started, ok: false, expected });
        if (!expected)
          ctx.logger.error('tool failed', { tool: def.name, error: errorMessage(error) });
        return {
          isError: true,
          content: [
            {
              type: 'text' as const,
              text: expected ? errorMessage(error) : `${def.name} failed: internal error`,
            },
          ],
        };
      }
    },
  );
}

type SpecFile = 'sections.json' | 'commands.json';

function specDocument(spec: Spec, file?: SpecFile): unknown {
  const sections = {
    meta: spec.meta,
    sections: spec.sections,
    builtinSections: spec.builtinSections,
  };
  const commands = {
    meta: spec.meta,
    commands: spec.commands,
    resourceMembers: spec.resourceMembers,
    functions: spec.functions,
  };
  if (file === 'sections.json') return sections;
  return file === 'commands.json' ? commands : { ...sections, ...commands };
}

function specFileOf(name: string): SpecFile {
  if (name !== 'sections.json' && name !== 'commands.json') {
    throw new ToolError(`unknown spec file "${name}"`);
  }
  return name;
}

const SPEC_FILES: SpecFile[] = ['sections.json', 'commands.json'];

/** One read of a spec resource; `dll_ref` is resolved like a tool's, falling back with the note in `_spec`. */
async function readSpecResource(
  ctx: ToolContext,
  uri: URL,
  dllRef: string | undefined,
  file?: SpecFile,
) {
  const resolved = await resolveSpec(ctx, dllRef);
  const document = specDocument(resolved.spec, file) as Record<string, unknown>;
  return {
    contents: [
      {
        uri: uri.href,
        mimeType: 'application/json',
        text: JSON.stringify({ ...document, _spec: specBlock(resolved) }),
      },
    ],
  };
}

function registerResources(server: McpServer, ctx: ToolContext): void {
  const description =
    'DLL-derived spec of a DLL version (dll_ref: latest-release by default, latest-prerelease, head, a tag, a commit prefix or bundled)';
  const options = { description, mimeType: 'application/json' };

  server.registerResource('spec', 'xxmi://spec', options, (uri) =>
    readSpecResource(ctx, uri, undefined),
  );
  server.registerResource(
    'spec-version',
    new ResourceTemplate('xxmi://spec/{dll_ref}', { list: undefined }),
    options,
    (uri, variables) => readSpecResource(ctx, uri, String(variables['dll_ref'])),
  );
  server.registerResource(
    'spec-file',
    new ResourceTemplate('xxmi://spec/{dll_ref}/{file}', {
      list: () => ({
        resources: SPEC_FILES.map((file) => ({
          uri: `xxmi://spec/latest-release/${file}`,
          name: `spec ${file}`,
          description:
            'Spec of the latest release (falls back to the bundled spec while it builds)',
          mimeType: 'application/json',
        })),
      }),
    }),
    {
      description:
        'sections.json (sections, keys) or commands.json (commands, members tagged resource or shader, functions) of a DLL version',
      mimeType: 'application/json',
    },
    (uri, variables) =>
      readSpecResource(
        ctx,
        uri,
        String(variables['dll_ref']),
        specFileOf(String(variables['file'])),
      ),
  );
}

export function createMcpServer(ctx: ToolContext): McpServer {
  const server = new McpServer(
    { name: 'xxmi-mcp', version: VERSION },
    { instructions: INSTRUCTIONS },
  );
  for (const def of TOOLS) register(server, ctx, def as unknown as ToolDefinition);
  registerResources(server, ctx);
  return server;
}
