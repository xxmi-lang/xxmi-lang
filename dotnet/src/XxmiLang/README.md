# XxmiLang

Lint and analyse **XXMI / 3DMigoto mod `.ini` files** from C#. This is a typed client for the
[xxmi-lang](https://github.com/xxmi-lang/xxmi-lang) tools: the language rules live in the `xxmi`
binary (the same one the CLI, VS Code and Kate use), so results always match what the editor shows.

## Install

```bash
dotnet add package XxmiLang                    # net10.0; trim and Native AOT compatible (brings XxmiLang.Core)
dotnet add package XxmiLang.Native.win-x64      # the xxmi binary; add one per platform you ship
```

Runtime packages: `XxmiLang.Native.` + `win-x64`, `win-arm64`, `linux-x64`, `linux-arm64`,
`osx-x64`, `osx-arm64`. Without one, set `XxmiOptions.ExecutablePath` or the `XXMI_PATH`
environment variable, or put `xxmi` on `PATH`.

## Lint

```csharp
using XxmiLang;

IXxmiLinter linter = new XxmiLinter();
LintResult result = await linter.LintAsync(new[] { @"C:\XXMI\ZZMI\Mods\MyMod" });
// also LintFileAsync(path) and LintTextAsync(path, text) for unsaved text
foreach (var (path, d) in result.AllDiagnostics)
    Console.WriteLine($"{path}:{d.Range.Start.Line + 1}  {d.Severity}  {d.Id}  {d.Message}");
```

## Language service

```csharp
await using var service = new XxmiLanguageService();
await service.StartAsync(@"C:\XXMI\ZZMI\Mods\MyMod");
await service.OpenDocumentAsync(@"C:\XXMI\ZZMI\Mods\MyMod\mod.ini");

var diagnostics = await service.GetDiagnosticsAsync(path);
var hover = await service.GetHoverAsync(path, line: 4, character: 1);       // 0-based
var definition = await service.GetDefinitionAsync(path, 5, 10);
var completions = await service.GetCompletionsAsync(path, 10, 6);
```

Also: `FindReferencesAsync`, `GetOutlineAsync`, `PrepareRenameAsync`, `RenameAsync`,
`GetCodeActionsAsync`, `UpdateDocumentAsync`, `CloseDocumentAsync`, and the events
`DiagnosticsPublished`, `ServerExited` and `LogMessage`.

Full guide: [docs/10-dotnet.md](https://github.com/xxmi-lang/xxmi-lang/blob/main/docs/10-dotnet.md).
License: GPL-3.0-or-later.
