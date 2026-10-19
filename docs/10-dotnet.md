# 10 — .NET library (`XxmiLang`)

Add XXMI / 3DMigoto `.ini` linting and analysis to your own C# project. This is a delivered extra outside the milestones in `09-roadmap.md`.

## What it is

A thin, typed client over the `xxmi` binary. It does not parse, resolve or lint anything itself (`CLAUDE.md`: "One core, many frontends"): it starts `xxmi lint --format json` or `xxmi lsp --stdio`, speaks their protocols and gives you C# types. Results are identical to the CLI and the editors, and a new rule or spec update reaches your app by updating the runtime package.

- **`XxmiLinter`**: lint files and folders, one process per call. Use it for batch work and CI-like checks.
- **`XxmiLanguageService`**: one long-lived language server. Use it for an editor-like experience: diagnostics for text you hold in memory, hover, go to definition, references, completion, outline, rename, quick fixes.

Not included: a `XxmiQuery` wrapper. `xxmi query` is not implemented yet (`04-cli.md`, "Implemented so far"); it will get one when it exists.

## Packages

| Package | Contents |
|---|---|
| `XxmiLang` | The managed library. Targets `netstandard2.0` (covers .NET Framework 4.7.2+ and .NET Core 2.0+) and `net8.0`. Depends on `System.Text.Json` (and `Microsoft.Bcl.AsyncInterfaces`) on `netstandard2.0` only |
| `XxmiLang.Native.win-x64`, `.win-arm64`, `.linux-x64`, `.linux-arm64`, `.osx-x64`, `.osx-arm64` | `runtimes/<rid>/native/xxmi[.exe]`: the self-contained single-file binary (65 to 100 MB; 24 to 38 MB as a package; no Node or Bun needed). Each package is far below nuget.org's 250 MB limit |
| `XxmiLang.All` | Meta package: `XxmiLang` plus all six runtime packages |

All have the repo's release version (`packages/core/src/version.ts`), so the library, the binary and the CLI always match. Licence: GPL-3.0-or-later.

## Install

```bash
dotnet add package XxmiLang
dotnet add package XxmiLang.Native.win-x64        # the platform(s) you run on
# or, for an app that must run anywhere:
dotnet add package XxmiLang.All
```

`XxmiLang.All` copies all six binaries (about 500 MB) into a framework-dependent app's `bin/` and publish folder. If you publish per platform (`dotnet publish -r linux-x64`) NuGet only takes the matching one, so prefer the single runtime package for a shipped app and keep `XxmiLang.All` for development tools and prototypes.

### From a release or a build, without nuget.org

Every GitHub release has the `.nupkg` files attached.

```bash
mkdir ~/xxmi-nupkgs && cd ~/xxmi-nupkgs
gh release download --repo xxmi-lang/xxmi-lang --pattern '*.nupkg'
dotnet nuget add source ~/xxmi-nupkgs --name xxmi-local     # once per machine (or put a nuget.config next to your .sln)
cd path/to/your/project
dotnet add package XxmiLang
dotnet add package XxmiLang.Native.osx-arm64
```

From a clone of this repo, to pack what you have built:

```bash
pnpm build && pnpm build:binaries --target host      # needs Bun
node scripts/pack-dotnet.ts --host --out ~/xxmi-nupkgs  # XxmiLang + this machine's runtime package
node scripts/smoke-dotnet.ts --feed ~/xxmi-nupkgs       # optional: installs them in a fresh app and lints a fixture
```

(`node scripts/pack-dotnet.ts` without `--host` packs all six runtime packages and `XxmiLang.All` from the binaries of `pnpm build:binaries`. NuGet caches packages by id and version: if you repack the same version, delete `~/.nuget/packages/xxmilang*` first.)

Or skip NuGet and reference the project: `<ProjectReference Include="…/xxmi-lang/dotnet/src/XxmiLang/XxmiLang.csproj" />` plus `XXMI_PATH` pointing at `dist/bin/xxmi`.

## Lint

```csharp
using XxmiLang;

LintResult result = await XxmiLinter.LintAsync(new[] { @"C:\XXMI\ZZMI\Mods\MyMod" });

foreach (var (path, d) in result.AllDiagnostics)
    Console.WriteLine($"{path}:{d.Range.Start.Line + 1}:{d.Range.Start.Character + 1}  {d.Severity}  {d.Id}  {d.Message}");

Console.WriteLine($"{result.Summary.Files} files, {result.ErrorCount} errors, {result.WarningCount} warnings");
```

- `LintAsync(IEnumerable<string> paths, LintOptions? options = null, CancellationToken ct = default)`, plus an overload for one path. Files and folders; folders are walked like the DLL does (`DISABLED*` is skipped unless named directly).
- `LintResult`: `Files` (`LintFile`: absolute `Path`, `Encoding`, `Diagnostics`), `Summary` (`Files`, `Error`, `Warning`, `Info`, `Hint`), `HasErrors`, `AllDiagnostics`.
- `XxmiDiagnostic`: `Id` (`XM201`), `Severity`, `Message`, `Range` (0-based line and UTF-16 column, `End` exclusive), `Fix` (`Title` and `Edits`, or null). Apply fixes with `TextEdits.Apply(text, fix.Edits)`.
- `LintOptions`: `Rules` (only these rule ids), `WorkingDirectory`, `ExecutablePath`.
- **Findings are not an error.** Exit code 1 (errors found) still returns a `LintResult`. Exit codes 2 and up (a path that doesn't exist, an internal error) throw `XxmiProcessException` with `ExitCode` and `StandardError`. A command line too long for Windows is split over several runs and merged.

## Language service

```csharp
await using var service = new XxmiLanguageService();
service.DiagnosticsPublished += (_, e) => Console.WriteLine($"{e.Path}: {e.Diagnostics.Count} diagnostics");
service.ServerExited += (_, e) => Console.WriteLine($"server died (exit {e.ExitCode}):\n{e.StandardError}");

// The mod folder, or better the XXMI install above it (the one with d3dx.ini), so library references resolve.
await service.StartAsync(@"C:\XXMI\ZZMI\Mods\MyMod");

var path = @"C:\XXMI\ZZMI\Mods\MyMod\mod.ini";
await service.OpenDocumentAsync(path);                    // text from disk; pass text to use your own
await service.UpdateDocumentAsync(path, editedText);      // in memory; never written to disk

IReadOnlyList<XxmiDiagnostic> diagnostics = await service.GetDiagnosticsAsync(path);
HoverResult? hover = await service.GetHoverAsync(path, line: 4, character: 1);       // 0-based
XxmiLocation? definition = await service.GetDefinitionAsync(path, 5, 10);             // may be in Core/Lib/…
IReadOnlyList<XxmiLocation> references = await service.FindReferencesAsync(path, 8, 5);
IReadOnlyList<CompletionItem> items = await service.GetCompletionsAsync(path, 10, 6);
IReadOnlyList<OutlineSymbol> outline = await service.GetOutlineAsync(path);

RenameCheck check = await service.PrepareRenameAsync(path, 8, 14);                    // CanRename, Placeholder, Message
if (check.CanRename)
{
    WorkspaceEdit edit = await service.RenameAsync(path, 8, 14, "Shared");           // Changes: path -> edits
}
IReadOnlyList<XxmiCodeAction> fixes = await service.GetCodeActionsAsync(path, diagnostics[0].Range);
```

- Every method is async, takes a `CancellationToken`, and works with local file paths and 0-based positions. The service is `IAsyncDisposable` (graceful `shutdown`/`exit`, kill after 3 s) and `IDisposable` (kill).
- `StartAsync(workspaceRoot)` starts `xxmi lsp --stdio`. The server indexes every `.ini` below the root in the background (up to 5,000 files); requests wait for that, so the first answer on a large folder is slower. `ServerVersion` and `ServerProcessId` are available once it runs.
- **`GetDiagnosticsAsync` is current.** It waits until the server has processed every change you sent, then returns the diagnostics for that text. Use the `DiagnosticsPublished` event if you prefer pushes; it is raised on a thread-pool thread, in order, and always carries the full list for one document.
- `XxmiRequestException` is the server refusing a request (`RenameAsync` of a library symbol). `PrepareRenameAsync` reports the same refusal as `CanRename = false` with a `Message`.
- Requests time out after `XxmiOptions.RequestTimeout` (60 s) with `TimeoutException`.
- **Crashes.** If the process ends without `StopAsync` or disposal, `ServerExited` is raised (exit code and the last stderr lines), pending and later calls throw `XxmiServerExitedException`, and `IsRunning` is false. `StartAsync` again starts a new process and re-opens every document with its latest text. `RestartAsync` does the same on demand. `StopAsync` forgets the documents.
- Calls on a document that isn't open throw `InvalidOperationException`; a service that isn't started does too.
- Thread safety: the service can be used from several threads. Document changes are sent in the order they take the internal lock, so don't update the same document from two threads at once.

## Where the binary comes from

In this order; the first match wins:

1. `XxmiOptions.ExecutablePath` (a file, or a folder containing `xxmi`).
2. The `XXMI_PATH` environment variable (same).
3. `runtimes/<rid>/native/xxmi[.exe]` next to `XxmiLang.dll` or the app (what the runtime packages install), and `xxmi[.exe]` itself next to them (what a `-r <rid>` publish produces). `<rid>` is `win`/`linux`/`osx` plus `x64`/`arm64`, from `RuntimeInformation`.
4. `xxmi` on `PATH`.

An explicit setting (1 or 2) that points at nothing is an error and doesn't fall through, so a typo can't silently run another binary. If nothing is found, `XxmiBinaryNotFoundException` lists every place that was tried (`Tried`). `XxmiBinaryLocator.Resolve()` runs the same lookup if you want to check it at startup.

What the runtime package does in your project (verified with a fresh app and a local feed, no `XXMI_PATH`):

| Command | Where `xxmi` ends up |
|---|---|
| `dotnet build`, `dotnet run` | `bin/<cfg>/<tfm>/runtimes/<rid>/native/xxmi` |
| `dotnet publish` | `publish/runtimes/<rid>/native/xxmi` |
| `dotnet publish -r <rid>`, also with `PublishSingleFile` | next to the app (`publish/xxmi`) |

NuGet does the copying; there is no custom MSBuild targets file.

## Platform notes

- **Linux and macOS: the exec bit.** NuGet doesn't restore Unix permissions reliably. Before the first run of a packaged binary the library sets the execute bit (`chmod u+x`; it needs to own the file, which it does in `~/.nuget/packages` and in your output folder). If you copy `xxmi` around yourself, `chmod +x` it. A failure to start produces an `XxmiProcessException` that says so.
- **macOS: Gatekeeper.** The binary is not signed or notarized. Files that arrive through NuGet aren't quarantined, so the packages work as is. A binary you download in a browser gets `com.apple.quarantine`, and macOS refuses to run it ("cannot be opened because the developer cannot be verified"): run `xattr -d com.apple.quarantine /path/to/xxmi` (or right-click, Open once). The same applies to the `.nupkg` files if you unzip one yourself from a browser download.
- **Windows.** `xxmi.exe` is unsigned, so SmartScreen may warn on a file downloaded in a browser ("Unblock" in Properties), and some antivirus products scan the first launch slowly. NuGet-delivered files aren't marked. Keep the app out of folders your antivirus watches closely if the first lint is slow.
- **Linux libc.** The binaries are built for glibc. Alpine (musl) isn't supported; use a glibc image.
- **Architectures.** x64 and arm64 only. On another machine `XxmiBinaryLocator.CurrentRuntimeIdentifier()` is null and the lookup reports "no xxmi build".
- **.NET Framework.** `netstandard2.0` works on .NET Framework 4.7.2+. SDK-style projects copy the runtime package like above; old `packages.config` projects don't copy `runtimes/` at all, so use `XXMI_PATH` or `ExecutablePath`.
- **Trimming and Native AOT** are not supported (the library uses reflection-based `System.Text.Json`). The binary is external anyway, so this rarely matters.
- **.NET 8 end of support** is November 2026. The `netstandard2.0` build keeps working on newer runtimes; add a `net10.0` target when it matters.

## Design choices

- **A thin client, not a port.** All language logic stays in `packages/core`. The library only builds command lines, parses the stable JSON (`packages/cli/schema/lint-output.v1.json`; `XxmiLinter` refuses a `version` it doesn't know) and speaks LSP.
- **A hand-written JSON-RPC client** (about 300 lines: `Content-Length` framing, requests with cancellation, notifications, answering server requests) instead of StreamJsonRpc. The surface needed is tiny, and StreamJsonRpc pulls in Nerdbank.Streams, Newtonsoft.Json and more, which is heavy for a library whose job is to be easy to add. It is covered by in-memory pipe tests.
- **The LSP over the CLI binary.** `xxmi lsp --stdio` (the CLI's subcommand; `xxmi-lsp` still exists for editors) means one 65 MB binary per platform instead of two.
- **No long-lived lint server for `XxmiLinter`.** One process per call costs a few hundred milliseconds of startup, and keeps the linter stateless. Use the language service when you lint repeatedly.

## Testing

`dotnet/` holds the solution `XxmiLang.sln`: the library, the packaging projects, `XxmiLang.Tests` (xUnit; lint, every language service request, crash and restart, binary resolution, the JSON-RPC layer) and `samples/XxmiLang.Sample`.

```bash
pnpm build && pnpm build:binaries --target host
XXMI_PATH=$PWD/dist/bin/xxmi dotnet test dotnet/                             # XxmiLang built for net8.0
XXMI_PATH=$PWD/dist/bin/xxmi dotnet test dotnet/ -p:XxmiLibraryTfm=netstandard2.0   # and for netstandard2.0

XXMI_PATH=$PWD/dist/bin/xxmi dotnet run --project dotnet/samples/XxmiLang.Sample -- lint fixtures/lsp/package/Mods/MyMod
XXMI_PATH=$PWD/dist/bin/xxmi dotnet run --project dotnet/samples/XxmiLang.Sample -- hover fixtures/lsp/package/Mods/MyMod/mod.ini 5 2
```

Without `XXMI_PATH`, the tests use `dist/bin/xxmi` if it exists. CI runs both test passes on Linux, Windows and macOS, then packs this machine's runtime package and installs it in a fresh app with no `XXMI_PATH` (`scripts/smoke-dotnet.ts`).

## Releasing

`release.yml` runs `scripts/pack-dotnet.ts` after the binaries are built, smoke-tests the packages on all three OSes, and attaches the `.nupkg` files to the GitHub release. If the repository has a `NUGET_API_KEY` secret, the last step also pushes them to nuget.org (the runtime packages and `XxmiLang` first, `XxmiLang.All` last); without it that step is skipped.
