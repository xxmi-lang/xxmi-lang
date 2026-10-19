using System.Collections.Generic;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace XxmiLang
{
    // The LSP messages the client sends, and the lint JSON it reads. All of it goes through WireJsonContext
    // (System.Text.Json source generation): no reflection, so the library stays trim and Native AOT safe.
    // Server answers are read from the JsonElement tree in LspParsing, because several LSP results are unions
    // (Location | Location[] | LocationLink[], string | MarkupContent, ...).

    internal sealed record EmptyObject;

    internal sealed record TextDocumentIdentifier(string Uri);

    internal sealed record VersionedTextDocumentIdentifier(string Uri, int Version);

    internal sealed record TextDocumentItem(string Uri, string LanguageId, int Version, string Text);

    internal sealed record DidOpenParams(TextDocumentItem TextDocument);

    /// <summary>A change without a range replaces the whole text, whatever sync mode the server asked for.</summary>
    internal sealed record FullTextChange(string Text);

    internal sealed record DidChangeParams(VersionedTextDocumentIdentifier TextDocument, IReadOnlyList<FullTextChange> ContentChanges);

    internal sealed record DidCloseParams(TextDocumentIdentifier TextDocument);

    internal sealed record DocumentSymbolParams(TextDocumentIdentifier TextDocument);

    internal sealed record PositionParams(TextDocumentIdentifier TextDocument, TextPosition Position);

    internal sealed record ReferenceContext(bool IncludeDeclaration);

    internal sealed record ReferenceParams(TextDocumentIdentifier TextDocument, TextPosition Position, ReferenceContext Context);

    internal sealed record RenameParams(TextDocumentIdentifier TextDocument, TextPosition Position, string NewName);

    /// <summary>The server's own diagnostics, handed back untouched (they carry the fix in <c>data</c>).</summary>
    internal sealed record CodeActionContext(IReadOnlyList<JsonElement> Diagnostics);

    internal sealed record CodeActionParams(TextDocumentIdentifier TextDocument, TextRange Range, CodeActionContext Context);

    internal sealed record CancelParams(long Id);

    internal sealed record ClientInfo(string Name, string? Version);

    internal sealed record WorkspaceFolder(string Uri, string Name);

    internal sealed record HoverCapabilities(IReadOnlyList<string> ContentFormat);

    internal sealed record DocumentSymbolCapabilities(bool HierarchicalDocumentSymbolSupport);

    internal sealed record CompletionItemCapabilities(IReadOnlyList<string> DocumentationFormat);

    internal sealed record CompletionCapabilities(CompletionItemCapabilities CompletionItem);

    internal sealed record RenameCapabilities(bool PrepareSupport);

    internal sealed record TextDocumentCapabilities(
        EmptyObject PublishDiagnostics,
        HoverCapabilities Hover,
        DocumentSymbolCapabilities DocumentSymbol,
        CompletionCapabilities Completion,
        RenameCapabilities Rename);

    internal sealed record ClientCapabilities(TextDocumentCapabilities TextDocument);

    internal sealed record InitializeParams(
        int ProcessId,
        ClientInfo ClientInfo,
        string? RootUri,
        IReadOnlyList<WorkspaceFolder>? WorkspaceFolders,
        ClientCapabilities Capabilities);

    // The wire shape of packages/cli/schema/lint-output.v1.json (the diagnostics are the public XxmiDiagnostic).
    internal sealed record LintOutputJson(int Version, IReadOnlyList<LintFileJson>? Files);

    internal sealed record LintFileJson(string Path, string Encoding, IReadOnlyList<XxmiDiagnostic>? Diagnostics);

    /// <summary>
    /// Everything the client writes, plus the lint output it reads. Nulls are written (LSP wants <c>"rootUri": null</c>),
    /// except where a type says otherwise.
    /// </summary>
    [JsonSourceGenerationOptions(PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase, PropertyNameCaseInsensitive = true)]
    [JsonSerializable(typeof(EmptyObject))]
    [JsonSerializable(typeof(InitializeParams))]
    [JsonSerializable(typeof(DidOpenParams))]
    [JsonSerializable(typeof(DidChangeParams))]
    [JsonSerializable(typeof(DidCloseParams))]
    [JsonSerializable(typeof(DocumentSymbolParams))]
    [JsonSerializable(typeof(PositionParams))]
    [JsonSerializable(typeof(ReferenceParams))]
    [JsonSerializable(typeof(RenameParams))]
    [JsonSerializable(typeof(CodeActionParams))]
    [JsonSerializable(typeof(CancelParams))]
    [JsonSerializable(typeof(LintOutputJson))]
    internal sealed partial class WireJsonContext : JsonSerializerContext
    {
    }
}
