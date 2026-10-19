using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization.Metadata;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    /// <summary>
    /// A long-lived <c>xxmi lsp --stdio</c> process with typed requests: diagnostics, hover, definition,
    /// references, completion, outline, rename and quick fixes. Positions are 0-based; paths are local file paths.
    /// </summary>
    /// <remarks>
    /// Documents you open are held in memory by the server and never written to disk. If the server process
    /// ends on its own, <see cref="ServerExited"/> is raised, pending and later calls throw
    /// <see cref="XxmiServerExitedException"/>, and <see cref="StartAsync"/> starts a new process and re-opens
    /// every document with its latest text.
    /// </remarks>
    public sealed class XxmiLanguageService : IXxmiLanguageService, IDisposable
    {
        private static readonly StringComparer PathComparer =
            RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal;

        private readonly XxmiOptions _options;
        private readonly SemaphoreSlim _lifecycle = new SemaphoreSlim(1, 1);
        private readonly SemaphoreSlim _documentLock = new SemaphoreSlim(1, 1);
        private readonly Dictionary<string, DocumentState> _documents = new Dictionary<string, DocumentState>(PathComparer);
        private readonly object _eventLock = new object();
        private Task _events = Task.CompletedTask;
        private Session? _session;
        private string? _workspaceRoot;
        private volatile bool _disposed;

        /// <param name="options">Where the binary is, the working directory and the request timeout; null: find it as <see cref="XxmiBinaryLocator"/> does.</param>
        public XxmiLanguageService(XxmiOptions? options = null)
        {
            _options = options ?? new XxmiOptions();
        }

        /// <summary>A server published diagnostics for a document (always the complete list for it). Raised on a thread-pool thread, in order.</summary>
        public event EventHandler<DiagnosticsPublishedEventArgs>? DiagnosticsPublished;

        /// <summary>The server process ended without <see cref="StopAsync"/> or disposal asking it to.</summary>
        public event EventHandler<ServerExitedEventArgs>? ServerExited;

        /// <summary>The server logged something (its stderr, or a <c>window/logMessage</c>).</summary>
        public event EventHandler<ServerLogEventArgs>? LogMessage;

        /// <summary>True while the server process is up and the connection is open.</summary>
        public bool IsRunning => _session is { Connection: { IsClosed: false } };

        /// <summary>The version the server reported when it started; null before that.</summary>
        public string? ServerVersion => _session?.Version;

        /// <summary>Process id of the server, or null when none is running.</summary>
        public int? ServerProcessId => IsRunning ? _session?.ProcessId : null;

        /// <summary>
        /// Starts <c>xxmi lsp --stdio</c> for a workspace folder. The server indexes every .ini file below the folder
        /// in the background; requests wait for that. Pass the folder of the mod, or your whole XXMI install (the one with
        /// <c>d3dx.ini</c>) so references into the installed libraries resolve. Without a folder the server only knows the
        /// documents you open and the bundled ZZMI library.
        /// </summary>
        /// <exception cref="InvalidOperationException">The service is already running.</exception>
        /// <exception cref="XxmiBinaryNotFoundException">There is no xxmi executable.</exception>
        public async Task StartAsync(string? workspaceRoot = null, CancellationToken cancellationToken = default)
        {
            ThrowIfDisposed();
            await _lifecycle.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                await StartCoreAsync(workspaceRoot is null ? null : Path.GetFullPath(workspaceRoot), cancellationToken).ConfigureAwait(false);
            }
            finally
            {
                _lifecycle.Release();
            }
        }

        /// <summary>Stops the server (if running) and starts a new one for the same workspace, re-opening the documents.</summary>
        public async Task RestartAsync(CancellationToken cancellationToken = default)
        {
            ThrowIfDisposed();
            await _lifecycle.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                await StopCoreAsync(clearDocuments: false).ConfigureAwait(false);
                await StartCoreAsync(_workspaceRoot, cancellationToken).ConfigureAwait(false);
            }
            finally
            {
                _lifecycle.Release();
            }
        }

        /// <summary>Shuts the server down politely (LSP shutdown/exit; killed after a few seconds) and forgets the open documents.</summary>
        public async Task StopAsync(CancellationToken cancellationToken = default)
        {
            await _lifecycle.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                await StopCoreAsync(clearDocuments: true).ConfigureAwait(false);
            }
            finally
            {
                _lifecycle.Release();
            }
        }

        /// <summary>Opens a document, or replaces its text if it is already open. Without <paramref name="text"/> the file is read from disk.</summary>
        public Task OpenDocumentAsync(string path, string? text = null, CancellationToken cancellationToken = default) =>
            SetTextAsync(path, text, cancellationToken);

        /// <summary>Replaces the text of a document (opens it if needed). Diagnostics follow; see <see cref="GetDiagnosticsAsync"/>.</summary>
        public Task UpdateDocumentAsync(string path, string text, CancellationToken cancellationToken = default)
        {
            if (text is null) throw new ArgumentNullException(nameof(text));
            return SetTextAsync(path, text, cancellationToken);
        }

        /// <summary>Closes a document; the server forgets the in-memory text and goes back to the file on disk.</summary>
        public async Task CloseDocumentAsync(string path, CancellationToken cancellationToken = default)
        {
            var full = Path.GetFullPath(path);
            await _documentLock.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                bool removed;
                lock (_documents) removed = _documents.Remove(full);
                if (!removed) return;
                var session = _session;
                if (session is null || session.Connection.IsClosed) return; // gone with the process
                var uri = FileUris.FromPath(full);
                session.Diagnostics.TryRemove(uri, out _);
                await session.Connection.NotifyAsync(
                    "textDocument/didClose",
                    new DidCloseParams(new TextDocumentIdentifier(uri)),
                    WireJsonContext.Default.DidCloseParams,
                    cancellationToken).ConfigureAwait(false);
            }
            finally
            {
                _documentLock.Release();
            }
        }

        /// <summary>
        /// The diagnostics for the current text of an open document. It waits until the server has processed every
        /// change sent so far, so the result is never from an older version.
        /// </summary>
        public async Task<IReadOnlyList<XxmiDiagnostic>> GetDiagnosticsAsync(string path, CancellationToken cancellationToken = default) =>
            (await GetPublishedAsync(path, cancellationToken).ConfigureAwait(false)).Diagnostics;

        /// <summary>Hover text at a position, or null.</summary>
        public async Task<HoverResult?> GetHoverAsync(string path, int line, int character, CancellationToken cancellationToken = default) =>
            LspParsing.Hover(await PositionRequestAsync("textDocument/hover", path, line, character, cancellationToken).ConfigureAwait(false));

        /// <summary>Where the symbol at a position is declared (a library file is fine), or null.</summary>
        public async Task<XxmiLocation?> GetDefinitionAsync(string path, int line, int character, CancellationToken cancellationToken = default) =>
            LspParsing.Location(await PositionRequestAsync("textDocument/definition", path, line, character, cancellationToken).ConfigureAwait(false));

        /// <summary>Every reference to the symbol at a position across the indexed workspace.</summary>
        public async Task<IReadOnlyList<XxmiLocation>> FindReferencesAsync(
            string path, int line, int character, bool includeDeclaration = true, CancellationToken cancellationToken = default)
        {
            var uri = OpenUri(path);
            var result = await RequestAsync(
                "textDocument/references",
                new ReferenceParams(new TextDocumentIdentifier(uri), new TextPosition(line, character), new ReferenceContext(includeDeclaration)),
                WireJsonContext.Default.ReferenceParams,
                cancellationToken).ConfigureAwait(false);
            if (result.ValueKind != JsonValueKind.Array) return Array.Empty<XxmiLocation>();
            return result.EnumerateArray().Select(LspParsing.Location).Where(l => l != null).Select(l => l!).ToList();
        }

        /// <summary>Completion suggestions at a position.</summary>
        public async Task<IReadOnlyList<CompletionItem>> GetCompletionsAsync(string path, int line, int character, CancellationToken cancellationToken = default)
        {
            var result = await PositionRequestAsync("textDocument/completion", path, line, character, cancellationToken).ConfigureAwait(false);
            if (result.ValueKind == JsonValueKind.Object && result.TryGetProperty("items", out var items)) result = items; // CompletionList
            if (result.ValueKind != JsonValueKind.Array) return Array.Empty<CompletionItem>();
            return result.EnumerateArray().Select(LspParsing.Completion).ToList();
        }

        /// <summary>The sections of a document grouped by kind.</summary>
        public async Task<IReadOnlyList<OutlineSymbol>> GetOutlineAsync(string path, CancellationToken cancellationToken = default)
        {
            var uri = OpenUri(path);
            var result = await RequestAsync(
                "textDocument/documentSymbol",
                new DocumentSymbolParams(new TextDocumentIdentifier(uri)),
                WireJsonContext.Default.DocumentSymbolParams,
                cancellationToken).ConfigureAwait(false);
            if (result.ValueKind != JsonValueKind.Array) return Array.Empty<OutlineSymbol>();
            return result.EnumerateArray().Select(LspParsing.Symbol).ToList();
        }

        /// <summary>
        /// Whether the name at a position can be renamed. Library symbols and non-symbols can't; <see cref="RenameCheck.Message"/> says why.
        /// </summary>
        public async Task<RenameCheck> PrepareRenameAsync(string path, int line, int character, CancellationToken cancellationToken = default)
        {
            JsonElement result;
            try
            {
                result = await PositionRequestAsync("textDocument/prepareRename", path, line, character, cancellationToken).ConfigureAwait(false);
            }
            catch (XxmiRequestException refused)
            {
                return new RenameCheck(false, null, null, refused.Message);
            }
            var range = result.ValueKind == JsonValueKind.Object && result.TryGetProperty("range", out var r) ? LspParsing.OptionalRange(r) : null;
            if (range is null) return new RenameCheck(false, null, null, "There is nothing to rename at this position.");
            return new RenameCheck(true, range, LspParsing.Str(result, "placeholder"), null);
        }

        /// <summary>The edits that rename the symbol at a position everywhere in the workspace. Apply them with <see cref="TextEdits.Apply"/>.</summary>
        /// <exception cref="XxmiRequestException">The rename is refused (see <see cref="PrepareRenameAsync"/>).</exception>
        public async Task<WorkspaceEdit> RenameAsync(string path, int line, int character, string newName, CancellationToken cancellationToken = default)
        {
            if (string.IsNullOrEmpty(newName)) throw new ArgumentException("The new name can't be empty.", nameof(newName));
            var uri = OpenUri(path);
            var result = await RequestAsync(
                "textDocument/rename",
                new RenameParams(new TextDocumentIdentifier(uri), new TextPosition(line, character), newName),
                WireJsonContext.Default.RenameParams,
                cancellationToken).ConfigureAwait(false);
            return LspParsing.WorkspaceEdit(result) ?? new WorkspaceEdit(new Dictionary<string, IReadOnlyList<TextEdit>>());
        }

        /// <summary>Quick fixes for the diagnostics that touch <paramref name="range"/>.</summary>
        public async Task<IReadOnlyList<XxmiCodeAction>> GetCodeActionsAsync(string path, TextRange range, CancellationToken cancellationToken = default)
        {
            var published = await GetPublishedAsync(path, cancellationToken).ConfigureAwait(false);
            var raw = new List<JsonElement>();
            for (var i = 0; i < published.Diagnostics.Count; i++)
            {
                if (Touches(published.Diagnostics[i].Range, range)) raw.Add(published.Raw[i]);
            }
            if (raw.Count == 0) return Array.Empty<XxmiCodeAction>();
            var result = await RequestAsync(
                "textDocument/codeAction",
                new CodeActionParams(new TextDocumentIdentifier(OpenUri(path)), range, new CodeActionContext(raw)),
                WireJsonContext.Default.CodeActionParams,
                cancellationToken).ConfigureAwait(false);
            if (result.ValueKind != JsonValueKind.Array) return Array.Empty<XxmiCodeAction>();
            return result.EnumerateArray().Select(LspParsing.CodeAction).ToList();
        }

        /// <summary>Quick fixes for the diagnostics on a line.</summary>
        public Task<IReadOnlyList<XxmiCodeAction>> GetCodeActionsAsync(string path, int line, CancellationToken cancellationToken = default) =>
            GetCodeActionsAsync(path, new TextRange(new TextPosition(line, 0), new TextPosition(line, int.MaxValue)), cancellationToken);

        public async ValueTask DisposeAsync()
        {
            if (_disposed) return;
            _disposed = true;
            try
            {
                await _lifecycle.WaitAsync().ConfigureAwait(false);
                try
                {
                    await StopCoreAsync(clearDocuments: true).ConfigureAwait(false);
                }
                finally
                {
                    _lifecycle.Release();
                }
            }
            catch (Exception)
            {
                // Disposal doesn't throw.
            }
        }

        /// <summary>Kills the server without the shutdown handshake. Prefer <see cref="DisposeAsync"/>.</summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            _session?.Kill();
            _session = null;
        }

        // ---- lifecycle ----------------------------------------------------------------------------------------

        private async Task StartCoreAsync(string? workspaceRoot, CancellationToken cancellationToken)
        {
            if (IsRunning) throw new InvalidOperationException("The language service is already running. Call StopAsync or RestartAsync first.");
            _session?.Kill(); // a crashed session's leftovers
            _session = null;

            var executable = XxmiBinaryLocator.Resolve(_options.ExecutablePath);
            var directory = _options.WorkingDirectory ?? workspaceRoot ?? Environment.CurrentDirectory;
            var start = ProcessRunner.CreateStartInfo(executable, new[] { "lsp", "--stdio" }, Directory.Exists(directory) ? directory : Environment.CurrentDirectory);
            var process = ProcessRunner.Start(start);
            var session = new Session(this, process);
            try
            {
                using (var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
                {
                    timeout.CancelAfter(_options.RequestTimeout);
                    var rootUri = workspaceRoot is null ? null : FileUris.FromPath(workspaceRoot);
                    var parameters = new InitializeParams(
                        Environment.ProcessId,
                        new ClientInfo("XxmiLang.NET", typeof(XxmiLanguageService).Assembly.GetName().Version?.ToString()),
                        rootUri,
                        rootUri is null ? null : new[] { new WorkspaceFolder(rootUri, Path.GetFileName(workspaceRoot!.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar))) },
                        new ClientCapabilities(new TextDocumentCapabilities(
                            new EmptyObject(),
                            new HoverCapabilities(new[] { "markdown", "plaintext" }),
                            new DocumentSymbolCapabilities(true),
                            new CompletionCapabilities(new CompletionItemCapabilities(new[] { "markdown", "plaintext" })),
                            new RenameCapabilities(true))));
                    JsonElement initialized;
                    try
                    {
                        initialized = await session.Connection.RequestAsync("initialize", parameters, WireJsonContext.Default.InitializeParams, timeout.Token).ConfigureAwait(false);
                    }
                    catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
                    {
                        throw new TimeoutException($"The language server didn't answer 'initialize' within {_options.RequestTimeout}.");
                    }
                    if (initialized.ValueKind == JsonValueKind.Object && initialized.TryGetProperty("serverInfo", out var info)) session.Version = LspParsing.Str(info, "version");
                    await session.Connection.NotifyAsync("initialized", new EmptyObject(), WireJsonContext.Default.EmptyObject, timeout.Token).ConfigureAwait(false);
                }

                _workspaceRoot = workspaceRoot;
                List<KeyValuePair<string, DocumentState>> documents;
                lock (_documents) documents = _documents.ToList();
                foreach (var document in documents)
                {
                    var uri = FileUris.FromPath(document.Key);
                    await session.Connection.NotifyAsync(
                        "textDocument/didOpen",
                        new DidOpenParams(new TextDocumentItem(uri, "migoto", document.Value.Version, document.Value.Text)),
                        WireJsonContext.Default.DidOpenParams,
                        cancellationToken).ConfigureAwait(false);
                }
                _session = session;
            }
            catch
            {
                session.Kill();
                throw;
            }
        }

        private async Task StopCoreAsync(bool clearDocuments)
        {
            var session = _session;
            _session = null;
            if (clearDocuments)
            {
                lock (_documents) _documents.Clear();
            }
            if (session is null) return;
            session.Stopping = true;
            if (!session.Connection.IsClosed)
            {
                try
                {
                    using (var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(3)))
                    {
                        await session.Connection.RequestAsync("shutdown", timeout.Token).ConfigureAwait(false);
                        await session.Connection.NotifyAsync("exit", timeout.Token).ConfigureAwait(false);
                    }
                }
                catch (Exception)
                {
                    // Not answering is what the kill below is for.
                }
            }
            await session.WaitForExitOrKillAsync(TimeSpan.FromSeconds(3)).ConfigureAwait(false);
        }

        // ---- requests -----------------------------------------------------------------------------------------

        private async Task SetTextAsync(string path, string? text, CancellationToken cancellationToken)
        {
            ThrowIfDisposed();
            var full = Path.GetFullPath(path);
            if (text is null) text = File.ReadAllText(full);
            await _documentLock.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                bool isOpen;
                int version;
                lock (_documents)
                {
                    isOpen = _documents.TryGetValue(full, out var existing);
                    version = (existing?.Version ?? 0) + 1;
                    _documents[full] = new DocumentState(text, version);
                }
                var connection = CurrentSession().Connection;
                var uri = FileUris.FromPath(full);
                if (isOpen)
                {
                    // A change without a range replaces the whole text, whatever sync mode the server asked for.
                    await connection.NotifyAsync(
                        "textDocument/didChange",
                        new DidChangeParams(new VersionedTextDocumentIdentifier(uri, version), new[] { new FullTextChange(text) }),
                        WireJsonContext.Default.DidChangeParams,
                        cancellationToken).ConfigureAwait(false);
                }
                else
                {
                    await connection.NotifyAsync(
                        "textDocument/didOpen",
                        new DidOpenParams(new TextDocumentItem(uri, "migoto", version, text)),
                        WireJsonContext.Default.DidOpenParams,
                        cancellationToken).ConfigureAwait(false);
                }
            }
            finally
            {
                _documentLock.Release();
            }
        }

        private Task<JsonElement> PositionRequestAsync(string method, string path, int line, int character, CancellationToken cancellationToken) =>
            RequestAsync(
                method,
                new PositionParams(new TextDocumentIdentifier(OpenUri(path)), new TextPosition(line, character)),
                WireJsonContext.Default.PositionParams,
                cancellationToken);

        /// <summary>The URI of a document that must be open, on a service that is running.</summary>
        private string OpenUri(string path)
        {
            ThrowIfDisposed();
            var full = Path.GetFullPath(path);
            lock (_documents)
            {
                if (!_documents.ContainsKey(full)) throw new InvalidOperationException($"{full} isn't open. Call OpenDocumentAsync first.");
            }
            CurrentSession();
            return FileUris.FromPath(full);
        }

        private async Task<Published> GetPublishedAsync(string path, CancellationToken cancellationToken)
        {
            var uri = OpenUri(path);
            var session = CurrentSession();
            // The server handles messages in order and runs a request only after the document updates queued
            // before it, publishing diagnostics for them first. Any request on the document is therefore a barrier:
            // when it is answered, the last publication we saw is the one for the current text.
            await RequestAsync(
                "textDocument/documentSymbol",
                new DocumentSymbolParams(new TextDocumentIdentifier(uri)),
                WireJsonContext.Default.DocumentSymbolParams,
                cancellationToken).ConfigureAwait(false);
            if (session.Diagnostics.TryGetValue(uri, out var published)) return published;
            // Nothing published yet (the server skips files it doesn't index): give it a moment, then report none.
            using (var grace = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
            {
                grace.CancelAfter(TimeSpan.FromSeconds(2));
                try
                {
                    return await session.WaitForPublicationAsync(uri, grace.Token).ConfigureAwait(false);
                }
                catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
                {
                    return new Published(Array.Empty<XxmiDiagnostic>(), Array.Empty<JsonElement>());
                }
            }
        }

        private async Task<JsonElement> RequestAsync<TParams>(string method, TParams parameters, JsonTypeInfo<TParams> parametersInfo, CancellationToken cancellationToken)
        {
            var session = CurrentSession();
            using (var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
            {
                timeout.CancelAfter(_options.RequestTimeout);
                try
                {
                    return await session.Connection.RequestAsync(method, parameters, parametersInfo, timeout.Token).ConfigureAwait(false);
                }
                catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
                {
                    throw new TimeoutException($"The language server didn't answer '{method}' within {_options.RequestTimeout}.");
                }
            }
        }

        private Session CurrentSession()
        {
            ThrowIfDisposed();
            var session = _session ?? throw new InvalidOperationException("The language service isn't running. Call StartAsync first.");
            if (session.Connection.IsClosed && session.ExitedException is { } exited) throw exited;
            return session;
        }

        private void ThrowIfDisposed()
        {
            if (_disposed) throw new ObjectDisposedException(nameof(XxmiLanguageService));
        }

        private static bool Touches(TextRange a, TextRange b) =>
            Compare(a.Start, b.End) <= 0 && Compare(b.Start, a.End) <= 0;

        private static int Compare(TextPosition a, TextPosition b) => a.Line != b.Line ? a.Line.CompareTo(b.Line) : a.Character.CompareTo(b.Character);

        // ---- events -------------------------------------------------------------------------------------------

        // Handlers run one after another on the thread pool, so a slow or blocking handler can't stall the read loop
        // (which would deadlock a handler that waits for a request) and still sees events in order.
        private void Raise(Action action)
        {
            lock (_eventLock)
            {
                _events = _events.ContinueWith(
                    _ =>
                    {
                        try
                        {
                            action();
                        }
                        catch (Exception)
                        {
                            // A handler's exception is the handler's problem.
                        }
                    },
                    CancellationToken.None,
                    TaskContinuationOptions.None,
                    TaskScheduler.Default);
            }
        }

        // ---- internals ----------------------------------------------------------------------------------------

        private sealed record DocumentState(string Text, int Version);

        private sealed class Published
        {
            public Published(IReadOnlyList<XxmiDiagnostic> diagnostics, IReadOnlyList<JsonElement> raw)
            {
                Diagnostics = diagnostics;
                Raw = raw;
            }

            public IReadOnlyList<XxmiDiagnostic> Diagnostics { get; }

            /// <summary>The server's own diagnostics, handed back in code action requests (they carry the fix in <c>data</c>).</summary>
            public IReadOnlyList<JsonElement> Raw { get; }
        }

        private sealed class Session
        {
            private readonly XxmiLanguageService _owner;
            private readonly object _tailLock = new object();
            private readonly Queue<string> _stderrTail = new Queue<string>();
            private readonly List<(string Uri, TaskCompletionSource<Published> Waiter)> _waiters = new List<(string, TaskCompletionSource<Published>)>();

            public Session(XxmiLanguageService owner, Process process)
            {
                _owner = owner;
                Process = process;
                ProcessId = process.Id;
                Connection = new JsonRpcConnection(process.StandardOutput.BaseStream, process.StandardInput.BaseStream)
                {
                    NotificationHandler = OnNotification,
                    RequestHandler = OnRequest,
                    CreateClosedException = CreateExitedExceptionAsync,
                };
                Connection.Closed += OnClosed;
                process.ErrorDataReceived += (_, e) => OnStderr(e.Data);
                process.BeginErrorReadLine();
                Connection.Start();
            }

            public Process Process { get; }

            public int ProcessId { get; }

            public JsonRpcConnection Connection { get; }

            public ConcurrentDictionary<string, Published> Diagnostics { get; } = new ConcurrentDictionary<string, Published>(StringComparer.Ordinal);

            public string? Version { get; set; }

            public volatile bool Stopping;

            public XxmiServerExitedException? ExitedException { get; private set; }

            public void Kill()
            {
                Stopping = true;
                Connection.Abort(new XxmiServerExitedException("The language server was stopped.", null, StandardErrorTail()));
                ProcessRunner.Kill(Process);
                Process.Dispose();
            }

            public async Task WaitForExitOrKillAsync(TimeSpan timeout)
            {
                var exited = await Task.Run(() => Process.WaitForExit((int)timeout.TotalMilliseconds)).ConfigureAwait(false);
                if (!exited) ProcessRunner.Kill(Process);
                Connection.Abort(new XxmiServerExitedException("The language server was stopped.", null, StandardErrorTail()));
                Process.Dispose();
            }

            public Task<Published> WaitForPublicationAsync(string uri, CancellationToken cancellationToken)
            {
                var waiter = new TaskCompletionSource<Published>(TaskCreationOptions.RunContinuationsAsynchronously);
                lock (_waiters) _waiters.Add((uri, waiter));
                if (Diagnostics.TryGetValue(uri, out var already)) return Task.FromResult(already);
                cancellationToken.Register(() => waiter.TrySetCanceled(cancellationToken));
                return waiter.Task;
            }

            public string StandardErrorTail()
            {
                lock (_tailLock) return string.Join(Environment.NewLine, _stderrTail);
            }

            private void OnStderr(string? line)
            {
                if (line is null) return;
                lock (_tailLock)
                {
                    _stderrTail.Enqueue(line);
                    while (_stderrTail.Count > 40) _stderrTail.Dequeue();
                }
                _owner.Raise(() => _owner.LogMessage?.Invoke(_owner, new ServerLogEventArgs(line, isError: true)));
            }

            private void OnNotification(string method, JsonElement parameters)
            {
                switch (method)
                {
                    case "textDocument/publishDiagnostics":
                    {
                        var uri = LspParsing.Str(parameters, "uri");
                        if (uri is null || !parameters.TryGetProperty("diagnostics", out var list) || list.ValueKind != JsonValueKind.Array) return;
                        var raw = list.EnumerateArray().Select(d => d.Clone()).ToList();
                        var parsed = raw.Select(LspParsing.Diagnostic).ToList();
                        var published = new Published(parsed, raw);
                        Diagnostics[uri] = published;
                        lock (_waiters)
                        {
                            foreach (var (waitUri, waiter) in _waiters.Where(w => w.Uri == uri).ToList())
                            {
                                waiter.TrySetResult(published);
                                _waiters.Remove((waitUri, waiter));
                            }
                        }
                        string path;
                        try
                        {
                            path = FileUris.ToPath(uri);
                        }
                        catch (ArgumentException)
                        {
                            return;
                        }
                        _owner.Raise(() => _owner.DiagnosticsPublished?.Invoke(_owner, new DiagnosticsPublishedEventArgs(path, parsed)));
                        break;
                    }
                    case "window/logMessage":
                    {
                        var message = LspParsing.Str(parameters, "message");
                        var isError = parameters.TryGetProperty("type", out var type) && type.ValueKind == JsonValueKind.Number && type.GetInt32() == 1;
                        if (message != null) _owner.Raise(() => _owner.LogMessage?.Invoke(_owner, new ServerLogEventArgs(message, isError)));
                        break;
                    }
                }
            }

            private static Task<JsonElement> OnRequest(string method, JsonElement parameters)
            {
                switch (method)
                {
                    case "workspace/configuration":
                        // One null per requested item: we have no settings.
                        var count = parameters.ValueKind == JsonValueKind.Object && parameters.TryGetProperty("items", out var items) && items.ValueKind == JsonValueKind.Array
                            ? items.GetArrayLength()
                            : 0;
                        using (var nulls = JsonDocument.Parse("[" + string.Join(",", Enumerable.Repeat("null", count)) + "]"))
                        {
                            return Task.FromResult(nulls.RootElement.Clone());
                        }
                    case "client/registerCapability":
                    case "client/unregisterCapability":
                    case "window/workDoneProgress/create":
                    case "window/showMessageRequest":
                        return Task.FromResult(default(JsonElement));
                    default:
                        throw new XxmiRequestException("Method not found: " + method, -32601);
                }
            }

            private async Task<Exception> CreateExitedExceptionAsync()
            {
                if (Stopping) return new XxmiServerExitedException("The language server was stopped.", null, StandardErrorTail());
                int? exitCode = null;
                var exited = await Task.Run(() => Process.WaitForExit(3000)).ConfigureAwait(false);
                if (!exited) ProcessRunner.Kill(Process);
                else exitCode = Process.ExitCode;
                var tail = StandardErrorTail();
                var message = "The xxmi language server exited unexpectedly"
                    + (exitCode.HasValue ? $" (exit code {exitCode.Value})" : string.Empty)
                    + ". Call StartAsync to start it again."
                    + (tail.Length > 0 ? Environment.NewLine + "Last stderr output:" + Environment.NewLine + tail : string.Empty);
                ExitedException = new XxmiServerExitedException(message, exitCode, tail);
                return ExitedException;
            }

            private void OnClosed(Exception exception)
            {
                lock (_waiters)
                {
                    foreach (var (_, waiter) in _waiters) waiter.TrySetException(exception);
                    _waiters.Clear();
                }
                if (Stopping || ExitedException is null) return;
                var exit = ExitedException;
                _owner.Raise(() => _owner.ServerExited?.Invoke(_owner, new ServerExitedEventArgs(exit.ExitCode, exit.StandardError)));
            }
        }
    }
}
