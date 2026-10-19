using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Pipes;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Xunit;

namespace XxmiLang.Tests;

public class FileUriTests
{
    [Theory]
    [InlineData("/Users/me/mods/a b/mod.ini", "file:///Users/me/mods/a%20b/mod.ini")]
    [InlineData("/tmp/#1/[x]/é.ini", "file:///tmp/%231/%5Bx%5D/%C3%A9.ini")]
    public void Unix_paths_round_trip(string path, string uri)
    {
        Assert.Equal(uri, FileUris.FromPath(path, windows: false));
        Assert.Equal(path, FileUris.ToPath(uri, windows: false));
    }

    [Theory]
    [InlineData(@"C:\Mods\a b\mod.ini", "file:///C:/Mods/a%20b/mod.ini")]
    [InlineData(@"\\host\share\mod.ini", "file://host/share/mod.ini")]
    public void Windows_paths_round_trip(string path, string uri)
    {
        Assert.Equal(uri, FileUris.FromPath(path, windows: true));
        Assert.Equal(path, FileUris.ToPath(uri, windows: true));
    }

    [Fact]
    public void An_editor_style_drive_uri_is_understood()
    {
        Assert.Equal(@"c:\Mods\mod.ini", FileUris.ToPath("file:///c%3A/Mods/mod.ini", windows: true));
    }

    [Fact]
    public void Other_schemes_are_rejected()
    {
        Assert.Throws<ArgumentException>(() => FileUris.ToPath("https://example.com/a.ini", windows: false));
    }
}

internal sealed record EchoParams(string Value);

internal sealed record NoteParams(string Text);

[JsonSourceGenerationOptions(PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase)]
[JsonSerializable(typeof(EchoParams))]
[JsonSerializable(typeof(NoteParams))]
[JsonSerializable(typeof(string))]
internal sealed partial class TestJsonContext : JsonSerializerContext
{
}

public class JsonRpcConnectionTests
{
    private static JsonElement Json(string raw)
    {
        using var document = JsonDocument.Parse(raw);
        return document.RootElement.Clone();
    }

    /// <summary>Two connected ends; what one writes the other reads.</summary>
    private sealed class Pair : IDisposable
    {
        private readonly AnonymousPipeServerStream _aToB = new AnonymousPipeServerStream(PipeDirection.Out);
        private readonly AnonymousPipeServerStream _bToA = new AnonymousPipeServerStream(PipeDirection.Out);
        private readonly AnonymousPipeClientStream _bIn;
        private readonly AnonymousPipeClientStream _aIn;

        public Pair()
        {
            _bIn = new AnonymousPipeClientStream(PipeDirection.In, _aToB.ClientSafePipeHandle);
            _aIn = new AnonymousPipeClientStream(PipeDirection.In, _bToA.ClientSafePipeHandle);
            A = new JsonRpcConnection(_aIn, _aToB);
            B = new JsonRpcConnection(_bIn, _bToA);
        }

        public JsonRpcConnection A { get; }

        public JsonRpcConnection B { get; }

        /// <summary>B's side goes away: its output closes (A reads end of input).</summary>
        public void CloseB() => _bToA.Dispose();

        public void Dispose()
        {
            A.Dispose();
            B.Dispose();
            _aToB.Dispose();
            _bToA.Dispose();
            _aIn.Dispose();
            _bIn.Dispose();
        }
    }

    [Fact]
    public async Task Requests_get_their_responses_and_server_requests_are_answered()
    {
        using var pair = new Pair();
        pair.B.RequestHandler = (method, parameters) =>
            method == "echo"
                ? Task.FromResult(Json("{\"got\":" + JsonSerializer.Serialize(parameters.GetProperty("value").GetString(), TestJsonContext.Default.String) + "}"))
                : throw new XxmiRequestException("nope", -32601);
        pair.A.Start();
        pair.B.Start();

        var result = await pair.A.RequestAsync("echo", new EchoParams("héllo ✓"), TestJsonContext.Default.EchoParams, CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(10));
        Assert.Equal("héllo ✓", result.GetProperty("got").GetString());

        var error = await Assert.ThrowsAsync<XxmiRequestException>(() => pair.A.RequestAsync("other", CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(10)));
        Assert.Equal(-32601, error.Code);
        Assert.Equal("nope", error.Message);
    }

    [Fact]
    public async Task Notifications_arrive_in_order_and_big_messages_survive()
    {
        using var pair = new Pair();
        var seen = new List<string>();
        var done = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        pair.B.NotificationHandler = (method, parameters) =>
        {
            seen.Add(parameters.GetProperty("text").GetString()!.Length.ToString());
            if (seen.Count == 3) done.TrySetResult(true);
        };
        pair.A.Start();
        pair.B.Start();

        foreach (var size in new[] { 3, 300_000, 5 })
            await pair.A.NotifyAsync("note", new NoteParams(new string('x', size)), TestJsonContext.Default.NoteParams, CancellationToken.None);

        await done.Task.WaitAsync(TimeSpan.FromSeconds(10));
        Assert.Equal(new[] { "3", "300000", "5" }, seen);
    }

    [Fact]
    public async Task Pending_requests_fail_when_the_other_side_goes_away()
    {
        using var pair = new Pair();
        pair.A.CreateClosedException = () => Task.FromResult<Exception>(new XxmiServerExitedException("gone", 7, "boom"));
        pair.A.Start();
        // B never answers.
        var pending = pair.A.RequestAsync("never", CancellationToken.None);

        pair.CloseB();

        var error = await Assert.ThrowsAsync<XxmiServerExitedException>(() => pending.WaitAsync(TimeSpan.FromSeconds(10)));
        Assert.Equal(7, error.ExitCode);
        Assert.True(pair.A.IsClosed);
        // Later requests fail the same way, at once.
        await Assert.ThrowsAsync<XxmiServerExitedException>(() => pair.A.RequestAsync("later", CancellationToken.None));
    }

    [Fact]
    public async Task A_cancelled_request_throws_and_tells_the_server()
    {
        using var pair = new Pair();
        var cancelled = new TaskCompletionSource<long>(TaskCreationOptions.RunContinuationsAsynchronously);
        pair.B.NotificationHandler = (method, parameters) =>
        {
            if (method == "$/cancelRequest") cancelled.TrySetResult(parameters.GetProperty("id").GetInt64());
        };
        pair.A.Start();
        pair.B.Start();
        using var cts = new CancellationTokenSource();

        var pending = pair.A.RequestAsync("slow", cts.Token);
        cts.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => pending);
        Assert.Equal(1, await cancelled.Task.WaitAsync(TimeSpan.FromSeconds(10)));
    }
}
