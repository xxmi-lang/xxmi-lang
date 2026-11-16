using System;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;

// A fake `xxmi lint` / `xxmi lsp --stdio`. Its behaviour is the first line of a file named "mode" in the working directory
// (the environment is shared by parallel tests, the working directory is not):
//   normal           answers like a healthy server
//   silent           reads its input and never answers
//   deaf             never reads its input
//   exit             exits at once, without a word
//   exit-after-init  LSP: answers initialize, then exits on the next message without replying
//   mute-after-init  LSP: answers initialize, then ignores everything (also shutdown)
//   deaf-after-init  LSP: answers initialize, then stops reading its input
//   stderr-flood     answers normally while writing megabytes to stderr
//   uri-variant      LSP: publishes diagnostics under another spelling of the document's URI
//   slow-exit        LSP: on exit, writes a file named "exited" after a short delay (a kill before that leaves no file)
var mode = File.Exists("mode") ? File.ReadAllText("mode").Trim() : "normal";
var isLsp = args.Length > 0 && args[0] == "lsp";

if (mode == "exit") return 0;
if (mode == "deaf") Thread.Sleep(Timeout.Infinite);
if (mode == "stderr-flood") StartFlooding();

return isLsp ? Lsp() : Lint();

static void StartFlooding()
{
    var thread = new Thread(() =>
    {
        var line = new string('x', 40_000);
        for (var i = 0; i < 200; i++) Console.Error.WriteLine(line);
    });
    thread.IsBackground = true;
    thread.Start();
}

int Lint()
{
    using var input = Console.OpenStandardInput();
    input.CopyTo(Stream.Null);
    if (mode == "silent") Thread.Sleep(Timeout.Infinite);
    Console.Out.Write("{\"version\":1,\"files\":[]}");
    return 0;
}

int Lsp()
{
    using var input = Console.OpenStandardInput();
    var output = Console.OpenStandardOutput();
    var initialized = false;
    while (ReadMessage(input) is { } message)
    {
        using (message)
        {
            var root = message.RootElement;
            var method = root.TryGetProperty("method", out var m) ? m.GetString() : null;
            var hasId = root.TryGetProperty("id", out var id);
            if (mode == "silent") continue;
            if (initialized && mode == "exit-after-init") return 0;
            if (initialized && mode == "mute-after-init") continue;

            switch (method)
            {
                case "initialize":
                    initialized = true;
                    Reply(output, id, "{\"capabilities\":{},\"serverInfo\":{\"name\":\"fake\",\"version\":\"0.0.0\"}}");
                    if (mode == "deaf-after-init") Thread.Sleep(Timeout.Infinite);
                    break;
                case "exit":
                    if (mode == "slow-exit")
                    {
                        Thread.Sleep(500);
                        File.WriteAllText("exited", string.Empty);
                    }

                    return 0;
                case "textDocument/didOpen":
                case "textDocument/didChange":
                    PublishDiagnostics(output, root.GetProperty("params"));
                    break;
                default:
                    if (hasId) Reply(output, id, method == "textDocument/documentSymbol" ? "[]" : "null");
                    break;
            }
        }
    }
    return 0;
}

void PublishDiagnostics(Stream output, JsonElement parameters)
{
    var document = parameters.GetProperty("textDocument");
    var uri = document.GetProperty("uri").GetString()!;
    if (mode == "uri-variant") uri = Respell(uri);
    var diagnostic = "{\"range\":{\"start\":{\"line\":0,\"character\":0},\"end\":{\"line\":0,\"character\":1}},\"severity\":1,\"code\":\"XM201\",\"message\":\"fake\"}";
    Send(output, "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":" + JsonSerializer.Serialize(uri) + ",\"diagnostics\":[" + diagnostic + "]}}");
}

// What an editor-style client sends for the same file: lower-case drive letter with an encoded colon, lower-case percent escapes.
static string Respell(string uri)
{
    var lowered = Regex.Replace(uri, "%[0-9A-F]{2}", match => match.Value.ToLowerInvariant());
    return Regex.Replace(lowered, "^file:///([A-Za-z]):", match => "file:///" + match.Groups[1].Value.ToLowerInvariant() + "%3a");
}

static void Reply(Stream output, JsonElement id, string resultJson) =>
    Send(output, "{\"jsonrpc\":\"2.0\",\"id\":" + id.GetRawText() + ",\"result\":" + resultJson + "}");

static void Send(Stream output, string json)
{
    var body = Encoding.UTF8.GetBytes(json);
    var header = Encoding.ASCII.GetBytes("Content-Length: " + body.Length + "\r\n\r\n");
    output.Write(header);
    output.Write(body);
    output.Flush();
}

static JsonDocument? ReadMessage(Stream input)
{
    var length = -1;
    while (true)
    {
        var line = ReadLine(input);
        if (line is null) return null;
        if (line.Length == 0) break;
        if (line.StartsWith("Content-Length:", StringComparison.OrdinalIgnoreCase)) length = int.Parse(line.Substring(15).Trim());
    }
    var buffer = new byte[length];
    for (var read = 0; read < length;)
    {
        var count = input.Read(buffer, read, length - read);
        if (count == 0) return null;
        read += count;
    }
    return JsonDocument.Parse(buffer);
}

static string? ReadLine(Stream input)
{
    var line = new StringBuilder();
    while (true)
    {
        var b = input.ReadByte();
        if (b < 0) return null;
        if (b == '\n') return line.ToString().TrimEnd('\r');
        line.Append((char)b);
    }
}
