using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Text.Json;
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

public class ArgumentQuotingTests
{
    [Theory]
    [InlineData("--format", "--format")]
    [InlineData("a b", "\"a b\"")]
    [InlineData("", "\"\"")]
    [InlineData("say \"hi\"", "\"say \\\"hi\\\"\"")]
    [InlineData(@"C:\dir with space\", "\"C:\\dir with space\\\\\"")]
    public void Arguments_are_quoted_for_CommandLineToArgv(string argument, string expected)
    {
        var joined = ProcessRunner.JoinArguments(new[] { argument });

        Assert.Equal(expected, joined);
    }
}

public class TextEditTests
{
    private static TextEdit Edit(int l1, int c1, int l2, int c2, string text) =>
        new TextEdit(new TextRange(new TextPosition(l1, c1), new TextPosition(l2, c2)), text);

    [Fact]
    public void Edits_refer_to_the_original_text_in_any_order()
    {
        var result = TextEdits.Apply("one\ntwo\nthree", new[] { Edit(2, 0, 2, 5, "3"), Edit(0, 0, 0, 3, "1") });

        Assert.Equal("1\ntwo\n3", result);
    }

    [Fact]
    public void Inserts_and_CRLF_work()
    {
        Assert.Equal("a\r\nXb\r\n", TextEdits.Apply("a\r\nb\r\n", new[] { Edit(1, 0, 1, 0, "X") }));
    }

    [Fact]
    public void Overlapping_edits_are_refused()
    {
        Assert.Throws<ArgumentException>(() => TextEdits.Apply("abcdef", new[] { Edit(0, 0, 0, 4, "x"), Edit(0, 2, 0, 5, "y") }));
    }
}

public class JsonRpcConnectionTests
{
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
            Task.FromResult<object?>(method == "echo" ? new { got = parameters.GetProperty("value").GetString() } : throw new XxmiRequestException("nope", -32601));
        pair.A.Start();
        pair.B.Start();

        var result = await pair.A.RequestAsync("echo", new { value = "héllo ✓" }, CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(10));
        Assert.Equal("héllo ✓", result.GetProperty("got").GetString());

        var error = await Assert.ThrowsAsync<XxmiRequestException>(() => pair.A.RequestAsync("other", null, CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(10)));
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
            await pair.A.NotifyAsync("note", new { text = new string('x', size) }, CancellationToken.None);

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
        var pending = pair.A.RequestAsync("never", null, CancellationToken.None);

        pair.CloseB();

        var error = await Assert.ThrowsAsync<XxmiServerExitedException>(() => pending.WaitAsync(TimeSpan.FromSeconds(10)));
        Assert.Equal(7, error.ExitCode);
        Assert.True(pair.A.IsClosed);
        // Later requests fail the same way, at once.
        await Assert.ThrowsAsync<XxmiServerExitedException>(() => pair.A.RequestAsync("later", null, CancellationToken.None));
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

        var pending = pair.A.RequestAsync("slow", null, cts.Token);
        cts.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => pending);
        Assert.Equal(1, await cancelled.Task.WaitAsync(TimeSpan.FromSeconds(10)));
    }
}
