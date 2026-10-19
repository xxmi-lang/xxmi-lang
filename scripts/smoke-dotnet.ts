/**
 * Proves the NuGet packages work from a fresh project (docs/10-dotnet.md): creates a console app that
 * references XxmiLang and this machine's XxmiLang.Native.<rid> from a local feed, with no XXMI_PATH and
 * no xxmi on PATH (CI has none), and has it lint a fixture. The binary must come from the package.
 *
 *   node scripts/smoke-dotnet.ts [--feed dist/nupkg]
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

const REPO = resolve(import.meta.dirname, '..');
const { values } = parseArgs({ options: { feed: { type: 'string' } } });
const feed = resolve(values.feed ?? join(REPO, 'dist/nupkg'));

const oses: Partial<Record<NodeJS.Platform, string>> = {
  win32: 'win',
  linux: 'linux',
  darwin: 'osx',
};
const rid = `${oses[process.platform] ?? process.platform}-${process.arch}`;
const lib = readdirSync(feed).find((f) => /^XxmiLang\.\d.*\.nupkg$/.test(f));
if (!lib) throw new Error(`no XxmiLang.<version>.nupkg in ${feed}`);
const version = lib.slice('XxmiLang.'.length, -'.nupkg'.length);

const dir = mkdtempSync(join(tmpdir(), 'xxmi-smoke-'));
try {
  const app = join(dir, 'app');
  mkdirSync(app);
  const xml = (s: string): string => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;');
  writeFileSync(
    join(app, 'nuget.config'),
    `<configuration>
  <packageSources>
    <clear />
    <add key="local" value="${xml(feed)}" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
`,
  );
  writeFileSync(
    join(app, 'Smoke.csproj'),
    `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <RollForward>Major</RollForward>
    <RestorePackagesPath>${xml(join(dir, 'packages'))}</RestorePackagesPath>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="XxmiLang" Version="${version}" />
    <PackageReference Include="XxmiLang.Native.${rid}" Version="${version}" />
  </ItemGroup>
</Project>
`,
  );
  writeFileSync(
    join(app, 'Program.cs'),
    `using System;
using System.Linq;
using XxmiLang;

var result = await XxmiLinter.LintAsync(args[0]);
Console.WriteLine($"files={result.Files.Count} errors={result.ErrorCount}");
Console.WriteLine("binary=" + XxmiBinaryLocator.Resolve());

await using var service = new XxmiLanguageService();
await service.StartAsync(args[0]);
Console.WriteLine("server=" + service.ServerVersion);
`,
  );

  // No XXMI_PATH: the binary has to come from the package (CI has no xxmi on PATH).
  const env = { ...process.env };
  delete env.XXMI_PATH;
  const fixture = join(REPO, 'fixtures/lsp/package/Mods/MyMod');
  const out = execFileSync('dotnet', ['run', '--project', app, '--', fixture], {
    cwd: dirname(app),
    env,
    encoding: 'utf8',
  });
  console.log(out);
  if (!/files=2 errors=0/.test(out)) throw new Error('unexpected lint result');
  // The app's own bin folder (the temp dir may print as /private/var on macOS).
  if (!/^binary=.*xxmi-smoke-.*[\\/]bin[\\/].*xxmi(\.exe)?$/m.test(out))
    throw new Error('the binary did not come from the package');
  if (!/server=\d+\.\d+\.\d+/.test(out)) throw new Error('the language server did not start');
  console.log(`OK: XxmiLang ${version} + XxmiLang.Native.${rid} work from a local feed`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
