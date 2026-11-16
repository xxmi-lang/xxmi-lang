using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Xunit;

namespace XxmiLang.Tests;

/// <summary>
/// What the library does when the process it drives misbehaves: every case must end with an exception or a result,
/// never with a wait that has no end. The outer guard turns a regression into a failure instead of a stuck test run.
/// </summary>
public class HangGuardTests
{
    private static readonly TimeSpan Guard = TimeSpan.FromSeconds(60);
    private static readonly TimeSpan ShortTimeout = TimeSpan.FromSeconds(2);

    [Fact]
    public async Task A_lint_process_that_never_answers_is_killed_and_reported()
    {
        using var server = FakeServer.InMode("silent", processTimeout: ShortTimeout);

        var error = await Assert.ThrowsAsync<XxmiProcessException>(() => new XxmiLinter(server.Options).LintFileAsync(TestEnvironment.ModIni).WaitAsync(Guard));

        Assert.Contains("did not finish", error.Message);
    }

    [Fact]
    public async Task A_lint_process_that_never_reads_its_input_is_killed_and_reported()
    {
        using var server = FakeServer.InMode("deaf", processTimeout: ShortTimeout);
        var text = new string('a', 4_000_000); // far beyond any pipe buffer

        var error = await Assert.ThrowsAsync<XxmiProcessException>(() => new XxmiLinter(server.Options).LintTextAsync(TestEnvironment.ModIni, text).WaitAsync(Guard));

        Assert.Contains("did not finish", error.Message);
    }

    [Fact]
    public async Task A_lint_process_that_exits_without_output_is_reported()
    {
        using var server = FakeServer.InMode("exit");

        await Assert.ThrowsAsync<XxmiProcessException>(() => new XxmiLinter(server.Options).LintFileAsync(TestEnvironment.ModIni).WaitAsync(Guard));
    }

    [Fact]
    public async Task A_lint_process_that_floods_stderr_still_completes()
    {
        using var server = FakeServer.InMode("stderr-flood");

        var result = await new XxmiLinter(server.Options).LintFileAsync(TestEnvironment.ModIni).WaitAsync(Guard);

        Assert.Empty(result.Files);
    }

    [Fact]
    public async Task A_server_that_never_answers_initialize_times_out_on_start()
    {
        using var server = FakeServer.InMode("silent", requestTimeout: ShortTimeout);
        await using var service = new XxmiLanguageService(server.Options);

        await Assert.ThrowsAsync<TimeoutException>(() => service.StartAsync(server.Directory).WaitAsync(Guard));

        Assert.False(service.IsRunning);
    }

    [Fact]
    public async Task A_server_that_never_reads_its_input_times_out_on_start()
    {
        using var server = FakeServer.InMode("deaf", requestTimeout: ShortTimeout);
        await using var service = new XxmiLanguageService(server.Options);

        await Assert.ThrowsAsync<TimeoutException>(() => service.StartAsync(server.Directory).WaitAsync(Guard));
    }

    [Fact]
    public async Task A_server_that_goes_silent_times_out_requests_and_is_killed_on_dispose()
    {
        using var server = FakeServer.InMode("mute-after-init");
        var service = new XxmiLanguageService(server.Options);
        await service.StartAsync(server.Directory).WaitAsync(Guard);
        var pid = service.ServerProcessId!.Value;
        await service.OpenDocumentAsync(TestEnvironment.ModIni).WaitAsync(Guard);
        server.Options.RequestTimeout = ShortTimeout; // read per request, so the start above was not held to it

        await Assert.ThrowsAsync<TimeoutException>(() => service.GetHoverAsync(TestEnvironment.ModIni, 4, 1).WaitAsync(Guard));
        await service.DisposeAsync().AsTask().WaitAsync(Guard); // the shutdown handshake is never answered: killed after ShutdownTimeout

        Assert.True(await ProcessGone(pid));
    }

    [Fact]
    public async Task An_infinite_ShutdownTimeout_waits_for_the_server_to_exit_instead_of_killing_it()
    {
        using var server = FakeServer.InMode("slow-exit");
        server.Options.ShutdownTimeout = Timeout.InfiniteTimeSpan;
        var service = new XxmiLanguageService(server.Options);
        await service.StartAsync(server.Directory).WaitAsync(Guard);
        var pid = service.ServerProcessId!.Value;

        await service.DisposeAsync().AsTask().WaitAsync(Guard);

        Assert.True(await ProcessGone(pid));
        Assert.True(File.Exists(Path.Combine(server.Directory, "exited")));
        Assert.False(service.IsRunning);
    }

    [Fact]
    public async Task A_server_that_exits_without_replying_fails_the_pending_request()
    {
        using var server = FakeServer.InMode("exit-after-init");
        await using var service = new XxmiLanguageService(server.Options);
        await service.StartAsync(server.Directory).WaitAsync(Guard);
        await service.OpenDocumentAsync(TestEnvironment.ModIni).WaitAsync(Guard);

        await Assert.ThrowsAsync<XxmiServerExitedException>(() => service.GetHoverAsync(TestEnvironment.ModIni, 4, 1).WaitAsync(Guard));

        Assert.False(service.IsRunning);
    }

    [Fact]
    public async Task A_server_that_stops_reading_is_killed_when_a_write_stalls()
    {
        using var server = FakeServer.InMode("deaf-after-init");
        await using var service = new XxmiLanguageService(server.Options);
        await service.StartAsync(server.Directory).WaitAsync(Guard);
        var pid = service.ServerProcessId!.Value;
        var text = new string('a', 8_000_000); // far beyond any pipe buffer

        await Assert.ThrowsAsync<XxmiServerExitedException>(() => service.OpenDocumentAsync(TestEnvironment.ModIni, text).WaitAsync(Guard));

        Assert.False(service.IsRunning);
        Assert.True(await ProcessGone(pid));
    }

    [Fact]
    public async Task A_server_that_floods_stderr_keeps_working()
    {
        using var server = FakeServer.InMode("stderr-flood");
        await using var service = new XxmiLanguageService(server.Options);
        await service.StartAsync(server.Directory).WaitAsync(Guard);

        await service.OpenDocumentAsync(TestEnvironment.ModIni).WaitAsync(Guard);

        var diagnostic = Assert.Single(await service.GetDiagnosticsAsync(TestEnvironment.ModIni).WaitAsync(Guard));
        Assert.Equal("fake", diagnostic.Message);
    }

    [Fact]
    public async Task Diagnostics_echoed_under_another_spelling_of_the_URI_still_arrive()
    {
        using var server = FakeServer.InMode("uri-variant");
        await using var service = new XxmiLanguageService(server.Options);
        await service.StartAsync(server.Directory).WaitAsync(Guard);
        // Non-ASCII, so the percent-escapes (and on Windows the drive letter) are spelled differently by the fake.
        var path = System.IO.Path.Combine(server.Directory, "模组", "mod.ini");

        await service.OpenDocumentAsync(path, "[CommandListLocal]\n").WaitAsync(Guard);

        var diagnostic = Assert.Single(await service.GetDiagnosticsAsync(path).WaitAsync(Guard));
        Assert.Equal("fake", diagnostic.Message);
    }

    private static async Task<bool> ProcessGone(int pid)
    {
        for (var attempt = 0; attempt < 50; attempt++)
        {
            try
            {
                using var process = Process.GetProcessById(pid);
                if (process.HasExited) return true;
            }
            catch (ArgumentException)
            {
                return true;
            }
            await Task.Delay(100);
        }
        return false;
    }
}
