using System;
using System.Collections.Concurrent;
using System.IO;
using System.Text;
using System.Buffers;
using System.Text.Json;
using System.Text.Json.Serialization.Metadata;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    /// <summary>
    /// JSON-RPC 2.0 over a pair of streams with LSP's <c>Content-Length</c> framing. Just enough for a
    /// language client: requests with cancellation, notifications both ways, and answering server requests.
    /// </summary>
    internal sealed class JsonRpcConnection : IDisposable
    {
        private static readonly byte[] HeaderEnd = { (byte)'\r', (byte)'\n', (byte)'\r', (byte)'\n' };

        private readonly Stream _input;
        private readonly Stream _output;
        private readonly SemaphoreSlim _writeLock = new SemaphoreSlim(1, 1);
        private readonly ConcurrentDictionary<long, TaskCompletionSource<JsonElement>> _pending = new ConcurrentDictionary<long, TaskCompletionSource<JsonElement>>();
        private long _nextId;
        private volatile Exception? _closedWith;
        private int _closeStarted;
        private byte[] _buffer = new byte[64 * 1024];
        private int _start;
        private int _end;

        public JsonRpcConnection(Stream input, Stream output)
        {
            _input = input;
            _output = output;
        }

        /// <summary>Answers a request from the server. Return the result (an undefined <see cref="JsonElement"/> sends <c>null</c>); throw to send an error.</summary>
        public Func<string, JsonElement, Task<JsonElement>>? RequestHandler { get; set; }

        /// <summary>Called on the read loop for every notification, in order. Must be quick and must not keep the element.</summary>
        public Action<string, JsonElement>? NotificationHandler { get; set; }

        /// <summary>Builds the exception that pending and later requests fail with once the streams end.</summary>
        public Func<Task<Exception>>? CreateClosedException { get; set; }

        /// <summary>Raised once, after pending requests were failed, when the input ended or broke.</summary>
        public event Action<Exception>? Closed;

        public Task Completion { get; private set; } = Task.CompletedTask;

        public bool IsClosed => Volatile.Read(ref _closeStarted) != 0;

        public void Start()
        {
            Completion = Task.Run(ReadLoopAsync);
        }

        public Task<JsonElement> RequestAsync(string method, CancellationToken cancellationToken) =>
            RequestAsync(method, (w, id) => WriteRequest(w, id, method, null), cancellationToken);

        public Task<JsonElement> RequestAsync<TParams>(string method, TParams parameters, JsonTypeInfo<TParams> parametersInfo, CancellationToken cancellationToken) =>
            RequestAsync(method, (w, id) => WriteRequest(w, id, method, () => JsonSerializer.Serialize(w, parameters, parametersInfo)), cancellationToken);

        private async Task<JsonElement> RequestAsync(string method, Action<Utf8JsonWriter, long> write, CancellationToken cancellationToken)
        {
            ThrowIfClosed();
            var id = Interlocked.Increment(ref _nextId);
            var completion = new TaskCompletionSource<JsonElement>(TaskCreationOptions.RunContinuationsAsynchronously);
            _pending[id] = completion;
            if (IsClosed)
            {
                // Closed between the check and the registration: nobody will answer.
                _pending.TryRemove(id, out _);
                ThrowIfClosed();
            }
            using (cancellationToken.Register(() =>
            {
                if (_pending.TryRemove(id, out var removed))
                {
                    removed.TrySetCanceled(cancellationToken);
                    _ = NotifyAsync("$/cancelRequest", new CancelParams(id), WireJsonContext.Default.CancelParams, CancellationToken.None).ContinueWith(t => _ = t.Exception, TaskContinuationOptions.OnlyOnFaulted);
                }
            }))
            {
                try
                {
                    await SendAsync(w => write(w, id), cancellationToken).ConfigureAwait(false);
                }
                catch
                {
                    _pending.TryRemove(id, out _);
                    throw;
                }
                return await completion.Task.ConfigureAwait(false);
            }
        }

        public Task NotifyAsync(string method, CancellationToken cancellationToken)
        {
            ThrowIfClosed();
            return SendAsync(w => WriteNotification(w, method, null), cancellationToken);
        }

        public Task NotifyAsync<TParams>(string method, TParams parameters, JsonTypeInfo<TParams> parametersInfo, CancellationToken cancellationToken)
        {
            ThrowIfClosed();
            return SendAsync(w => WriteNotification(w, method, () => JsonSerializer.Serialize(w, parameters, parametersInfo)), cancellationToken);
        }

        // The envelope is written by hand; the params go through the caller's source-generated JsonTypeInfo.
        private static void WriteRequest(Utf8JsonWriter writer, long id, string method, Action? writeParams)
        {
            writer.WriteStartObject();
            writer.WriteString("jsonrpc", "2.0");
            writer.WriteNumber("id", id);
            writer.WriteString("method", method);
            WriteParams(writer, writeParams);
            writer.WriteEndObject();
        }

        private static void WriteNotification(Utf8JsonWriter writer, string method, Action? writeParams)
        {
            writer.WriteStartObject();
            writer.WriteString("jsonrpc", "2.0");
            writer.WriteString("method", method);
            WriteParams(writer, writeParams);
            writer.WriteEndObject();
        }

        private static void WriteParams(Utf8JsonWriter writer, Action? writeParams)
        {
            writer.WritePropertyName("params");
            if (writeParams is null) writer.WriteNullValue();
            else writeParams();
        }

        private static void WriteResult(Utf8JsonWriter writer, JsonElement id, JsonElement result)
        {
            writer.WriteStartObject();
            writer.WriteString("jsonrpc", "2.0");
            writer.WritePropertyName("id");
            id.WriteTo(writer);
            writer.WritePropertyName("result");
            if (result.ValueKind == JsonValueKind.Undefined) writer.WriteNullValue();
            else result.WriteTo(writer);
            writer.WriteEndObject();
        }

        private static void WriteError(Utf8JsonWriter writer, JsonElement id, int code, string message)
        {
            writer.WriteStartObject();
            writer.WriteString("jsonrpc", "2.0");
            writer.WritePropertyName("id");
            id.WriteTo(writer);
            writer.WriteStartObject("error");
            writer.WriteNumber("code", code);
            writer.WriteString("message", message);
            writer.WriteEndObject();
            writer.WriteEndObject();
        }

        private async Task SendAsync(Action<Utf8JsonWriter> write, CancellationToken cancellationToken)
        {
            var buffer = new ArrayBufferWriter<byte>();
            using (var writer = new Utf8JsonWriter(buffer)) write(writer);
            var body = buffer.WrittenMemory;
            var header = Encoding.ASCII.GetBytes("Content-Length: " + body.Length + "\r\n\r\n");
            await _writeLock.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                // Once the first byte is out the message must be complete: no cancellation from here on.
                await _output.WriteAsync(header, CancellationToken.None).ConfigureAwait(false);
                await _output.WriteAsync(body, CancellationToken.None).ConfigureAwait(false);
                await _output.FlushAsync().ConfigureAwait(false);
            }
            catch (Exception ex) when (ex is IOException || ex is ObjectDisposedException || ex is InvalidOperationException)
            {
                // The other side is gone. The read loop reports why; until it does, say so plainly.
                if (_closedWith is { } closed) throw closed;
                throw new IOException("The connection to the language server was lost: " + ex.Message, ex);
            }
            finally
            {
                _writeLock.Release();
            }
        }

        private void ThrowIfClosed()
        {
            if (_closedWith is { } closed) throw closed;
        }

        private async Task ReadLoopAsync()
        {
            Exception? cause = null;
            try
            {
                while (true)
                {
                    var length = await ReadHeaderAsync().ConfigureAwait(false);
                    if (length < 0) break;
                    if (!await FillAsync(length).ConfigureAwait(false)) break;
                    try
                    {
                        using (var document = JsonDocument.Parse(new ReadOnlyMemory<byte>(_buffer, _start, length)))
                        {
                            Dispatch(document.RootElement);
                        }
                    }
                    catch (JsonException)
                    {
                        // A malformed message from the server is skipped, not fatal.
                    }
                    _start += length;
                }
            }
            catch (Exception ex) when (ex is IOException || ex is ObjectDisposedException || ex is InvalidOperationException)
            {
                cause = ex;
            }
            await CloseAsync(cause).ConfigureAwait(false);
        }

        private async Task CloseAsync(Exception? cause)
        {
            if (Interlocked.Exchange(ref _closeStarted, 1) != 0) return;
            Exception exception;
            try
            {
                exception = CreateClosedException is { } create
                    ? await create().ConfigureAwait(false)
                    : new IOException("The connection was closed.", cause);
            }
            catch (Exception ex)
            {
                exception = ex;
            }
            _closedWith = exception;
            foreach (var key in _pending.Keys)
            {
                if (_pending.TryRemove(key, out var completion)) completion.TrySetException(exception);
            }
            Closed?.Invoke(exception);
        }

        /// <summary>Marks the connection closed on our side (we are shutting it down); pending requests fail with <paramref name="exception"/>.</summary>
        public void Abort(Exception exception)
        {
            if (Interlocked.Exchange(ref _closeStarted, 1) != 0) return;
            _closedWith = exception;
            foreach (var key in _pending.Keys)
            {
                if (_pending.TryRemove(key, out var completion)) completion.TrySetException(exception);
            }
        }

        private void Dispatch(JsonElement message)
        {
            var hasMethod = message.TryGetProperty("method", out var methodElement) && methodElement.ValueKind == JsonValueKind.String;
            var hasId = message.TryGetProperty("id", out var id) && id.ValueKind != JsonValueKind.Null;
            message.TryGetProperty("params", out var parameters);
            if (hasMethod && hasId)
            {
                var idCopy = id.Clone();
                var paramsCopy = parameters.ValueKind == JsonValueKind.Undefined ? default : parameters.Clone();
                var method = methodElement.GetString()!;
                _ = Task.Run(() => AnswerAsync(idCopy, method, paramsCopy));
            }
            else if (hasMethod)
            {
                try
                {
                    NotificationHandler?.Invoke(methodElement.GetString()!, parameters);
                }
                catch (Exception)
                {
                    // A handler bug must not end the session.
                }
            }
            else if (hasId && id.ValueKind == JsonValueKind.Number && id.TryGetInt64(out var responseId))
            {
                if (!_pending.TryRemove(responseId, out var completion)) return;
                if (message.TryGetProperty("error", out var error) && error.ValueKind == JsonValueKind.Object)
                {
                    var text = error.TryGetProperty("message", out var m) && m.ValueKind == JsonValueKind.String ? m.GetString()! : "The language server reported an error.";
                    var code = error.TryGetProperty("code", out var c) && c.TryGetInt32(out var number) ? number : 0;
                    completion.TrySetException(new XxmiRequestException(text, code));
                }
                else
                {
                    completion.TrySetResult(message.TryGetProperty("result", out var result) ? result.Clone() : default);
                }
            }
        }

        private async Task AnswerAsync(JsonElement id, string method, JsonElement parameters)
        {
            try
            {
                var handler = RequestHandler;
                if (handler is null) throw new XxmiRequestException("Method not found: " + method, -32601);
                var result = await handler(method, parameters).ConfigureAwait(false);
                await SendAsync(w => WriteResult(w, id, result), CancellationToken.None).ConfigureAwait(false);
            }
            catch (Exception ex) when (!(ex is OutOfMemoryException))
            {
                try
                {
                    var code = ex is XxmiRequestException request ? request.Code : -32603;
                    await SendAsync(w => WriteError(w, id, code, ex.Message), CancellationToken.None).ConfigureAwait(false);
                }
                catch (Exception)
                {
                    // The connection is gone; nothing to answer to.
                }
            }
        }

        /// <summary>Reads headers until the blank line; returns the Content-Length, or -1 at end of input.</summary>
        private async Task<int> ReadHeaderAsync()
        {
            while (true)
            {
                var end = IndexOf(HeaderEnd);
                if (end >= 0)
                {
                    var header = Encoding.ASCII.GetString(_buffer, _start, end - _start);
                    _start = end + HeaderEnd.Length;
                    foreach (var line in header.Split(new[] { "\r\n" }, StringSplitOptions.RemoveEmptyEntries))
                    {
                        var colon = line.IndexOf(':');
                        if (colon > 0 && line.Substring(0, colon).Trim().Equals("Content-Length", StringComparison.OrdinalIgnoreCase)
                            && int.TryParse(line.Substring(colon + 1).Trim(), out var length) && length >= 0)
                        {
                            return length;
                        }
                    }
                    throw new IOException("The language server sent a message without Content-Length.");
                }
                if (!await FillAsync(_end - _start + 1).ConfigureAwait(false)) return -1;
            }
        }

        private int IndexOf(byte[] needle)
        {
            for (var i = _start; i <= _end - needle.Length; i++)
            {
                var match = true;
                for (var j = 0; j < needle.Length && match; j++) match = _buffer[i + j] == needle[j];
                if (match) return i;
            }
            return -1;
        }

        /// <summary>Reads until at least <paramref name="wanted"/> unread bytes are buffered. False at end of input.</summary>
        private async Task<bool> FillAsync(int wanted)
        {
            while (_end - _start < wanted)
            {
                if (_start > 0 && (_start == _end || _end == _buffer.Length || _buffer.Length - _start < wanted))
                {
                    Buffer.BlockCopy(_buffer, _start, _buffer, 0, _end - _start);
                    _end -= _start;
                    _start = 0;
                }
                if (_buffer.Length < wanted || _end == _buffer.Length)
                {
                    Array.Resize(ref _buffer, Math.Max(wanted, _buffer.Length * 2));
                }
                var read = await _input.ReadAsync(_buffer, _end, _buffer.Length - _end).ConfigureAwait(false);
                if (read == 0) return false;
                _end += read;
            }
            return true;
        }

        public void Dispose()
        {
            Abort(new ObjectDisposedException(nameof(JsonRpcConnection)));
        }
    }
}
