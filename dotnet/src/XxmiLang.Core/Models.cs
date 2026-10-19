using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json.Serialization;

namespace XxmiLang
{
    /// <summary>A position in a text: 0-based line, 0-based column in UTF-16 code units (so it indexes a .NET string directly).</summary>
    public readonly record struct TextPosition(int Line, int Character);

    /// <summary>A range in a text; <see cref="End"/> is exclusive.</summary>
    public readonly record struct TextRange(TextPosition Start, TextPosition End);

    /// <summary>Replace <see cref="Range"/> with <see cref="NewText"/>.</summary>
    public sealed record TextEdit(TextRange Range, string NewText);

    /// <summary>A suggested fix for one diagnostic: edits to the same file.</summary>
    public sealed record QuickFix(string Title, IReadOnlyList<TextEdit> Edits);

    /// <summary>Same numbers as the LSP DiagnosticSeverity. In JSON: <c>"error"</c>, <c>"warning"</c>, <c>"info"</c>, <c>"hint"</c> (the CLI's words).</summary>
    [JsonConverter(typeof(JsonStringEnumConverter<DiagnosticSeverity>))]
    public enum DiagnosticSeverity
    {
        [JsonStringEnumMemberName("error")] Error = 1,
        [JsonStringEnumMemberName("warning")] Warning = 2,
        [JsonStringEnumMemberName("info")] Info = 3,
        [JsonStringEnumMemberName("hint")] Hint = 4,
    }

    /// <summary>One finding: a rule id such as <c>XM201</c>, where it is and what to do about it.</summary>
    public sealed record XxmiDiagnostic(
        string Id,
        DiagnosticSeverity Severity,
        string Message,
        TextRange Range,
        QuickFix? Fix = null);

    /// <summary>The diagnostics of one file.</summary>
    /// <param name="Path">Absolute path.</param>
    /// <param name="Encoding"><c>utf8</c>, <c>utf8bom</c> or <c>unknown</c>.</param>
    /// <param name="Diagnostics">In source order.</param>
    public sealed record LintFile(string Path, string Encoding, IReadOnlyList<XxmiDiagnostic> Diagnostics);

    /// <summary>How many files were linted and how many diagnostics of each severity there are.</summary>
    public sealed record LintSummary(int Files, int Error, int Warning, int Info, int Hint);

    /// <summary>The result of <see cref="IXxmiLinter"/>: one entry per linted file, plus counts.</summary>
    public sealed record LintResult(IReadOnlyList<LintFile> Files, LintSummary Summary)
    {
        [JsonIgnore]
        public int ErrorCount => Summary.Error;

        [JsonIgnore]
        public int WarningCount => Summary.Warning;

        /// <summary>True if any file has an error-severity diagnostic (the CLI's exit code 1).</summary>
        [JsonIgnore]
        public bool HasErrors => Summary.Error > 0;

        /// <summary>Every diagnostic of every file, with its file.</summary>
        [JsonIgnore]
        public IEnumerable<(string Path, XxmiDiagnostic Diagnostic)> AllDiagnostics =>
            Files.SelectMany(f => f.Diagnostics.Select(d => (f.Path, d)));
    }

    /// <summary>A place in a file.</summary>
    /// <param name="Path">Local file path.</param>
    /// <param name="Uri">The <c>file:</c> URI as the server wrote it.</param>
    public sealed record XxmiLocation(string Path, string Uri, TextRange Range);

    /// <summary>Hover text (Markdown) and the range it applies to.</summary>
    public sealed record HoverResult(string Markdown, TextRange? Range);

    /// <summary>Same numbers as the LSP CompletionItemKind.</summary>
    public enum CompletionItemKind
    {
        Text = 1, Method = 2, Function = 3, Constructor = 4, Field = 5, Variable = 6, Class = 7,
        Interface = 8, Module = 9, Property = 10, Unit = 11, Value = 12, Enum = 13, Keyword = 14,
        Snippet = 15, Color = 16, File = 17, Reference = 18, Folder = 19, EnumMember = 20,
        Constant = 21, Struct = 22, Event = 23, Operator = 24, TypeParameter = 25,
    }

    /// <summary>One completion suggestion.</summary>
    /// <param name="Label">What the editor lists.</param>
    /// <param name="NewText">What to insert in place of <see cref="Range"/>.</param>
    public sealed record CompletionItem(
        string Label,
        CompletionItemKind Kind,
        string? Detail,
        string? Documentation,
        string NewText,
        TextRange? Range);

    /// <summary>Same numbers as the LSP SymbolKind.</summary>
    public enum SymbolKind
    {
        File = 1, Module = 2, Namespace = 3, Package = 4, Class = 5, Method = 6, Property = 7,
        Field = 8, Constructor = 9, Enum = 10, Interface = 11, Function = 12, Variable = 13,
        Constant = 14, String = 15, Number = 16, Boolean = 17, Array = 18, Object = 19, Key = 20,
        Null = 21, EnumMember = 22, Struct = 23, Event = 24, Operator = 25, TypeParameter = 26,
    }

    /// <summary>
    /// A node of the document outline. Top-level nodes are section kinds (<c>CommandList</c>,
    /// <c>TextureOverride</c>, …, kind <see cref="SymbolKind.Namespace"/>); their children are the sections.
    /// </summary>
    public sealed record OutlineSymbol(
        string Name,
        SymbolKind Kind,
        TextRange Range,
        TextRange SelectionRange,
        IReadOnlyList<OutlineSymbol> Children);

    /// <summary>Edits to apply, per file.</summary>
    /// <param name="Changes">Local file path to the edits for that file.</param>
    public sealed record WorkspaceEdit(IReadOnlyDictionary<string, IReadOnlyList<TextEdit>> Changes);

    /// <summary>Result of <see cref="IXxmiLanguageService.PrepareRenameAsync"/>.</summary>
    /// <param name="CanRename">False for symbols declared in the package or the bundled library, and for non-symbols.</param>
    /// <param name="Range">The name that would change (when <see cref="CanRename"/>).</param>
    /// <param name="Placeholder">The current name (when <see cref="CanRename"/>).</param>
    /// <param name="Message">Why the rename is refused (when not <see cref="CanRename"/>).</param>
    public sealed record RenameCheck(bool CanRename, TextRange? Range, string? Placeholder, string? Message);

    /// <summary>A quick fix offered by the server.</summary>
    public sealed record XxmiCodeAction(string Title, string? Kind, bool IsPreferred, WorkspaceEdit? Edit);

    /// <summary>Passed to <see cref="IXxmiLanguageService.DiagnosticsPublished"/>.</summary>
    public sealed class DiagnosticsPublishedEventArgs : EventArgs
    {
        public DiagnosticsPublishedEventArgs(string path, IReadOnlyList<XxmiDiagnostic> diagnostics)
        {
            Path = path;
            Diagnostics = diagnostics;
        }

        /// <summary>The document the diagnostics are for (a full replacement of the previous ones).</summary>
        public string Path { get; }

        public IReadOnlyList<XxmiDiagnostic> Diagnostics { get; }
    }

    /// <summary>Passed to <see cref="IXxmiLanguageService.ServerExited"/>.</summary>
    public sealed class ServerExitedEventArgs : EventArgs
    {
        public ServerExitedEventArgs(int? exitCode, string standardError)
        {
            ExitCode = exitCode;
            StandardError = standardError;
        }

        public int? ExitCode { get; }

        /// <summary>The last lines the server wrote to stderr.</summary>
        public string StandardError { get; }
    }

    /// <summary>Passed to <see cref="IXxmiLanguageService.LogMessage"/>.</summary>
    public sealed class ServerLogEventArgs : EventArgs
    {
        public ServerLogEventArgs(string message, bool isError)
        {
            Message = message;
            IsError = isError;
        }

        public string Message { get; }

        public bool IsError { get; }
    }
}
