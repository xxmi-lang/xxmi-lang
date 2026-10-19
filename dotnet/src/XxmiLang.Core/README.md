# XxmiLang.Core

The shared definitions of the [xxmi-lang](https://github.com/xxmi-lang/xxmi-lang) .NET library for
**XXMI / 3DMigoto mod `.ini` files**: the result types (`XxmiDiagnostic`, `LintResult`, `TextRange`,
`QuickFix`, `TextEdit`, `HoverResult`, `XxmiLocation`, `CompletionItem`, `OutlineSymbol`,
`WorkspaceEdit`, ...), `TextEdits.Apply`, the interfaces `IXxmiLinter` and `IXxmiLanguageService`, and
a source-generated System.Text.Json context for them.

It has no process or file access and is trim and Native AOT compatible. Reference it alone to write
code or fakes against the interfaces. The implementation, which drives the bundled `xxmi` tool, is
[`XxmiLang`](https://www.nuget.org/packages/XxmiLang).

Full guide: [docs/10-dotnet.md](https://github.com/xxmi-lang/xxmi-lang/blob/main/docs/10-dotnet.md).
