/** Builds one `McpServer` with every tool and resource (one per request in stateless mode). */
import { McpServer, ResourceTemplate } from '@modelcontextprotocol/sdk/server/mcp.js';
import { VERSION } from '@xxmi-lang/core';
import { CatalogError } from '../catalog.ts';
import { errorMessage } from '../logger.ts';
import { SourceError } from '../sources.ts';
import type { ToolDefinition } from './define.ts';
import { libraryList, librarySymbol } from './library.ts';
import { lintIni } from './lint.ts';
import { findImplementationTool, readSourceTool, searchSourceTool } from './source.ts';
import { specList, specLookup } from './spec.ts';
import { ToolError, withTimeout, type ToolContext } from './shared.ts';
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

function specDocument(ctx: ToolContext, file: string, ref: string): unknown {
  const commit = ctx.spec.meta.dll.commit;
  if (ref !== 'latest' && !(ref.length >= 7 && commit.startsWith(ref.toLowerCase()))) {
    throw new ToolError(`unknown dll_ref "${ref}"; this server serves "latest" (${commit})`);
  }
  return file === 'sections.json'
    ? {
        meta: ctx.spec.meta,
        sections: ctx.spec.sections,
        builtinSections: ctx.spec.builtinSections,
      }
    : {
        meta: ctx.spec.meta,
        commands: ctx.spec.commands,
        resourceMembers: ctx.spec.resourceMembers,
        functions: ctx.spec.functions,
      };
}

function registerResources(server: McpServer, ctx: ToolContext): void {
  const commit = ctx.spec.meta.dll.commit;
  server.registerResource(
    'spec',
    new ResourceTemplate('xxmi://spec/{dll_ref}/{file}', {
      list: () => ({
        resources: ['sections.json', 'commands.json'].map((file) => ({
          uri: `xxmi://spec/latest/${file}`,
          name: `spec ${file}`,
          description: `Generated spec (XXMI-Libs-Package ${commit.slice(0, 7)})`,
          mimeType: 'application/json',
        })),
      }),
    }),
    {
      description:
        'DLL-derived spec: sections.json (sections, keys) and commands.json (commands, members, functions)',
      mimeType: 'application/json',
    },
    (uri, variables) => {
      const file = String(variables['file']);
      const ref = String(variables['dll_ref']);
      if (file !== 'sections.json' && file !== 'commands.json')
        throw new ToolError(`unknown spec file "${file}"`);
      return {
        contents: [
          {
            uri: uri.href,
            mimeType: 'application/json',
            text: JSON.stringify(specDocument(ctx, file, ref)),
          },
        ],
      };
    },
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
