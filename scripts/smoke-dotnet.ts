/**
 * Proves the NuGet packages work from a fresh project (docs/10-dotnet.md): creates a console app that
 * references XxmiLang and this machine's XxmiLang.Native.<rid> from a local feed (XxmiLang.Core comes in
 * with XxmiLang), with no XXMI_PATH and no xxmi on PATH (CI has none), and has it lint a folder, lint text,
 * and start the language server. The binary must come from the package.
 *
 *   node scripts/smoke-dotnet.ts [--feed dist/nupkg] [--aot]
 *
 * It runs the app with `dotnet run` (the binary is under bin/…/runtimes/<rid>/native). With `--aot` it also
 * publishes the same app with Native AOT (`-r <rid>`, any trim or AOT warning is an error) and runs the
 * native executable (the binary is next to it).
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

const REPO = resolve(import.meta.dirname, '..');
const { values } = parseArgs({
  options: { feed: { type: 'string' }, aot: { type: 'boolean', default: false } },
});
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
if (!readdirSync(feed).some((f) => /^XxmiLang\.Core\.\d.*\.nupkg$/.test(f)))
  throw new Error(`no XxmiLang.Core.<version>.nupkg in ${feed}`);

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
    <TargetFramework>net10.0</TargetFramework>
    <PublishAot>true</PublishAot>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
    <InvariantGlobalization>true</InvariantGlobalization>
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
using System.IO;
using System.Linq;
using XxmiLang;

IXxmiLinter linter = new XxmiLinter();
var result = await linter.LintAsync(new[] { args[0] });
Console.WriteLine($"files={result.Files.Count} errors={result.ErrorCount}");
Console.WriteLine("binary=" + XxmiBinaryLocator.Resolve());

var text = await linter.LintTextAsync(Path.Combine(args[0], "unsaved.ini"), "[CommandListX]\\nrun = CommandListY\\n", new LintOptions { Rules = new[] { "XM201" } });
Console.WriteLine("text=" + string.Join(",", text.AllDiagnostics.Select(d => d.Diagnostic.Id)));

await using IXxmiLanguageService service = new XxmiLanguageService();
await service.StartAsync(args[0]);
Console.WriteLine("server=" + service.ServerVersion);
`,
  );

  // No XXMI_PATH: the binary has to come from the package (CI has no xxmi on PATH).
  const env = { ...process.env };
  delete env.XXMI_PATH;
  const fixture = join(REPO, 'fixtures/lsp/package/Mods/MyMod');
  const check = (out: string, binaryPattern: RegExp, label: string): void => {
    console.log(out);
    if (!/files=2 errors=0/.test(out)) throw new Error(`${label}: unexpected lint result`);
    if (!binaryPattern.test(out))
      throw new Error(`${label}: the binary did not come from the package`);
    if (!/^text=XM201$/m.test(out)) throw new Error(`${label}: LintTextAsync gave no XM201`);
    if (!/server=\d+\.\d+\.\d+/.test(out))
      throw new Error(`${label}: the language server did not start`);
  };

  // The app's own bin folder (the temp dir may print as /private/var on macOS).
  check(
    execFileSync('dotnet', ['run', '--project', app, '--', fixture], {
      cwd: dirname(app),
      env,
      encoding: 'utf8',
    }),
    /^binary=.*xxmi-smoke-.*[\\/]bin[\\/].*xxmi(\.exe)?$/m,
    'dotnet run',
  );
  console.log(`OK: XxmiLang ${version} + XxmiLang.Native.${rid} work from a local feed`);

  if (values.aot) {
    const out = join(dir, 'aot');
    execFileSync(
      'dotnet',
      ['publish', app, '-c', 'Release', '-r', rid, '-o', out, '-p:PublishAot=true'],
      { cwd: dirname(app), env, stdio: 'inherit' },
    );
    const exe = join(out, process.platform === 'win32' ? 'Smoke.exe' : 'Smoke');
    check(
      execFileSync(exe, [fixture], { env, encoding: 'utf8' }),
      /^binary=.*xxmi-smoke-.*[\\/]aot[\\/]xxmi(\.exe)?$/m,
      'Native AOT',
    );
    console.log(`OK: the same app published with Native AOT (${rid}) works`);
  }
} finally {
  rmSync(dir, { recursive: true, force: true });
}
