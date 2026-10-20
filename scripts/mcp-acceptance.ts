/**
 * M6 acceptance (docs/09-roadmap.md): asks a running xxmi MCP server the three questions the
 * milestone names and checks the answers.
 *
 *   node scripts/mcp-acceptance.ts [URL]      (default http://localhost:3000/mcp)
 *
 * Talks plain JSON-RPC over the Streamable HTTP endpoint, so it needs no MCP client library and
 * works against any deployment. Waits for `/healthz` to report `ready` first.
 */
const url = new URL(process.argv[2] ?? 'http://localhost:3000/mcp');
const origin = url.origin;

interface RpcResponse {
  result?: {
    content?: { type: string; text: string }[];
    isError?: boolean;
    tools?: { name: string }[];
  };
  error?: { message: string };
}

let nextId = 1;

async function rpc(method: string, params: object = {}): Promise<RpcResponse> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream' },
    body: JSON.stringify({ jsonrpc: '2.0', id: nextId++, method, params }),
  });
  if (!res.ok) throw new Error(`${method}: HTTP ${res.status} ${await res.text()}`);
  return (await res.json()) as RpcResponse;
}

async function call(tool: string, args: object): Promise<string> {
  const response = await rpc('tools/call', { name: tool, arguments: args });
  if (response.error) throw new Error(`${tool}: ${response.error.message}`);
  const text = response.result?.content?.map((c) => c.text).join('\n') ?? '';
  if (response.result?.isError) throw new Error(`${tool} returned an error: ${text}`);
  return text;
}

async function waitUntilReady(timeoutMs = 300_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try {
      const health = (await (await fetch(`${origin}/healthz`)).json()) as { ready?: boolean };
      if (health.ready) return;
    } catch {
      // not up yet
    }
    if (Date.now() > deadline) throw new Error('server did not become ready');
    await new Promise((r) => setTimeout(r, 2000));
  }
}

const failures: string[] = [];

function check(question: string, answer: string, expected: (string | RegExp)[]): void {
  const missing = expected.filter((e) =>
    typeof e === 'string' ? !answer.includes(e) : !e.test(answer),
  );
  console.log(`${missing.length === 0 ? 'PASS' : 'FAIL'}  ${question}`);
  if (missing.length > 0) {
    failures.push(question);
    console.log(
      `      missing: ${missing.map(String).join(', ')}\n      got: ${answer.slice(0, 600)}`,
    );
  }
}

await waitUntilReady();
await rpc('initialize', {
  protocolVersion: '2025-06-18',
  capabilities: {},
  clientInfo: { name: 'xxmi-acceptance', version: '1' },
});

const tools = (await rpc('tools/list')).result?.tools?.map((t) => t.name) ?? [];
console.log(`tools: ${tools.join(', ')}`);

check(
  'what inputs does CommandList\\ZZMI\\SetTextures need',
  await call('library_symbol', { qualified_name: 'CommandList\\ZZMI\\SetTextures', game: 'zzmi' }),
  [
    'Resource\\ZZMI\\Diffuse',
    'Resource\\ZZMI\\NormalMap',
    'Resource\\ZZMI\\LightMap',
    'Resource\\ZZMI\\MaterialMap',
    'run = CommandList\\ZZMI\\SetTextures',
    /Resources\.ini:\d+/,
  ],
);

check(
  'where is `store` implemented in the DLL',
  await call('find_implementation', { feature: 'store' }),
  [
    /DirectX11\/CommandList\.cpp:\d+/,
    'ParseStoreCommand',
    'github.com/SpectrumQT/XXMI-Libs-Package/blob/',
  ],
);

const lintBad = await call('lint_ini', {
  files: {
    'Mods/Demo/mod.ini': '[TextureOverrideDemo]\nhash = 12345678\nrun = CommandListLocl\n',
  },
  game: 'zzmi',
});
check('lint this ini (unresolved run target)', lintBad, [/XM201/, 'Mods/Demo/mod.ini:3']);

const lintGood = await call('lint_ini', {
  files: {
    'Mods/Demo/mod.ini':
      '[TextureOverrideDemo]\nhash = 12345678\nrun = CommandList\\ZZMI\\SetTextures\n',
  },
  game: 'zzmi',
});
check('lint this ini (library reference resolves)', lintGood, [/0 errors/]);

if (failures.length > 0) {
  console.error(`\n${failures.length} acceptance check(s) failed`);
  process.exit(1);
}
console.log('\nall acceptance checks passed');
