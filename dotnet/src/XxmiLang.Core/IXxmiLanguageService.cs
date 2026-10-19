using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    /// <summary>
    /// A long-lived language server session: diagnostics for text you hold in memory, hover, definition, references,
    /// completion, outline, rename and quick fixes. Positions are 0-based; paths are local file paths. Implement it to
    /// fake the server in your own tests; <c>XxmiLanguageService</c> in the XxmiLang package drives <c>xxmi lsp --stdio</c>.
    /// </summary>
    /// <remarks>
    /// Documents you open are held in memory by the server and never written to disk. Calls on a document that isn't open
    /// throw <see cref="InvalidOperationException"/>, as do calls on a service that isn't started.
    /// </remarks>
    public interface IXxmiLanguageService : IAsyncDisposable
    {
        /// <summary>The server published diagnostics for a document (always the complete list for it). Raised in order, off the caller's thread.</summary>
        event EventHandler<DiagnosticsPublishedEventArgs>? DiagnosticsPublished;

        /// <summary>The server ended without <see cref="StopAsync"/> or disposal asking it to.</summary>
        event EventHandler<ServerExitedEventArgs>? ServerExited;

        /// <summary>The server logged something.</summary>
        event EventHandler<ServerLogEventArgs>? LogMessage;

        /// <summary>True while the server is up and the connection is open.</summary>
        bool IsRunning { get; }

        /// <summary>The version the server reported when it started; null before that.</summary>
        string? ServerVersion { get; }

        /// <summary>
        /// Starts the server for a workspace folder: the mod folder, or the whole XXMI install (the one with <c>d3dx.ini</c>) so
        /// references into the installed libraries resolve. Starting again after a crash re-opens every document with its latest text.
        /// </summary>
        Task StartAsync(string? workspaceRoot = null, CancellationToken cancellationToken = default);

        /// <summary>Stops the server (if running) and starts a new one for the same workspace, re-opening the documents.</summary>
        Task RestartAsync(CancellationToken cancellationToken = default);

        /// <summary>Shuts the server down politely and forgets the open documents.</summary>
        Task StopAsync(CancellationToken cancellationToken = default);

        /// <summary>Opens a document, or replaces its text if it is already open. Without <paramref name="text"/> the file is read from disk.</summary>
        Task OpenDocumentAsync(string path, string? text = null, CancellationToken cancellationToken = default);

        /// <summary>Replaces the text of a document (opens it if needed).</summary>
        Task UpdateDocumentAsync(string path, string text, CancellationToken cancellationToken = default);

        /// <summary>Closes a document; the server goes back to the file on disk.</summary>
        Task CloseDocumentAsync(string path, CancellationToken cancellationToken = default);

        /// <summary>The diagnostics for the current text of an open document: never from an older version of it.</summary>
        Task<IReadOnlyList<XxmiDiagnostic>> GetDiagnosticsAsync(string path, CancellationToken cancellationToken = default);

        /// <summary>Hover text at a position, or null.</summary>
        Task<HoverResult?> GetHoverAsync(string path, int line, int character, CancellationToken cancellationToken = default);

        /// <summary>Where the symbol at a position is declared (a library file is fine), or null.</summary>
        Task<XxmiLocation?> GetDefinitionAsync(string path, int line, int character, CancellationToken cancellationToken = default);

        /// <summary>Every reference to the symbol at a position across the indexed workspace.</summary>
        Task<IReadOnlyList<XxmiLocation>> FindReferencesAsync(string path, int line, int character, bool includeDeclaration = true, CancellationToken cancellationToken = default);

        /// <summary>Completion suggestions at a position.</summary>
        Task<IReadOnlyList<CompletionItem>> GetCompletionsAsync(string path, int line, int character, CancellationToken cancellationToken = default);

        /// <summary>The sections of a document grouped by kind.</summary>
        Task<IReadOnlyList<OutlineSymbol>> GetOutlineAsync(string path, CancellationToken cancellationToken = default);

        /// <summary>Whether the name at a position can be renamed; <see cref="RenameCheck.Message"/> says why not.</summary>
        Task<RenameCheck> PrepareRenameAsync(string path, int line, int character, CancellationToken cancellationToken = default);

        /// <summary>The edits that rename the symbol at a position everywhere in the workspace. Apply them with <see cref="TextEdits.Apply"/>.</summary>
        Task<WorkspaceEdit> RenameAsync(string path, int line, int character, string newName, CancellationToken cancellationToken = default);

        /// <summary>Quick fixes for the diagnostics that touch <paramref name="range"/>.</summary>
        Task<IReadOnlyList<XxmiCodeAction>> GetCodeActionsAsync(string path, TextRange range, CancellationToken cancellationToken = default);

        /// <summary>Quick fixes for the diagnostics on a line.</summary>
        Task<IReadOnlyList<XxmiCodeAction>> GetCodeActionsAsync(string path, int line, CancellationToken cancellationToken = default) =>
            GetCodeActionsAsync(path, new TextRange(new TextPosition(line, 0), new TextPosition(line, int.MaxValue)), cancellationToken);
    }
}
