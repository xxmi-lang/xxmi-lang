import type * as z from 'zod/v4';
import type { ToolContext, ToolOutput } from './shared.ts';

export interface ToolDefinition<S extends z.ZodRawShape = z.ZodRawShape> {
  name: string;
  description: string;
  input: S;
  run: (ctx: ToolContext, args: z.infer<z.ZodObject<S>>) => Promise<ToolOutput>;
}

/** Identity helper that ties `run`'s argument type to the input shape. */
export function defineTool<S extends z.ZodRawShape>(def: ToolDefinition<S>): ToolDefinition<S> {
  return def;
}
