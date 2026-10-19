using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading.Tasks;
using Xunit;

namespace XxmiLang.Tests;

public class LanguageServiceTests : IAsyncLifetime
{
    private readonly XxmiLanguageService _service = new XxmiLanguageService(TestEnvironment.Options());

    public async Task InitializeAsync() => await _service.StartAsync(TestEnvironment.Mod);

    public async Task DisposeAsync() => await _service.DisposeAsync();

    private static TextPosition At(int line, int character) => new TextPosition(line, character);

    [Fact]
    public async Task A_clean_mod_has_no_diagnostics_and_an_edit_introduces_XM201()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);
        Assert.Empty(await _service.GetDiagnosticsAsync(TestEnvironment.ModIni));

        var text = System.IO.File.ReadAllText(TestEnvironment.ModIni).Replace("\r\n", "\n");
        var broken = text.Replace("run = CommandListLocal\n", "run = CommandListLocl\n");
        await _service.UpdateDocumentAsync(TestEnvironment.ModIni, broken);

        var diagnostics = await _service.GetDiagnosticsAsync(TestEnvironment.ModIni);
        var xm201 = Assert.Single(diagnostics, d => d.Id == "XM201");
        Assert.Equal(DiagnosticSeverity.Error, xm201.Severity);
        Assert.Equal(At(4, 6), xm201.Range.Start);
        Assert.Contains("CommandListLocl", xm201.Message);

        // The quick fix repairs the typo.
        var actions = await _service.GetCodeActionsAsync(TestEnvironment.ModIni, xm201.Range);
        var action = Assert.Single(actions);
        Assert.Equal("Change to `CommandListLocal`", action.Title);
        var edits = Assert.Single(action.Edit!.Changes).Value;
        Assert.Equal(text, TextEdits.Apply(broken, edits));

        // And the server follows the document back to the clean state.
        await _service.UpdateDocumentAsync(TestEnvironment.ModIni, text);
        Assert.Empty(await _service.GetDiagnosticsAsync(TestEnvironment.ModIni));
    }

    [Fact]
    public async Task DiagnosticsPublished_is_raised_for_changes()
    {
        var published = new TaskCompletionSource<DiagnosticsPublishedEventArgs>(TaskCreationOptions.RunContinuationsAsynchronously);
        _service.DiagnosticsPublished += (_, e) =>
        {
            if (e.Diagnostics.Any(d => d.Id == "XM201")) published.TrySetResult(e);
        };
        var text = System.IO.File.ReadAllText(TestEnvironment.ModIni).Replace("\r\n", "\n").Replace("run = CommandListLocal\n", "run = CommandListLocl\n");

        await _service.OpenDocumentAsync(TestEnvironment.ModIni, text);

        var args = await published.Task.WaitAsync(TimeSpan.FromSeconds(20));
        Assert.Equal(TestEnvironment.ModIni, args.Path);
    }

    [Fact]
    public async Task Definition_goes_into_the_library()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);

        var location = await _service.GetDefinitionAsync(TestEnvironment.ModIni, 5, 10); // run = CommandList\Lib\Draw

        Assert.NotNull(location);
        Assert.Equal(TestEnvironment.LibIni, location!.Path);
        Assert.Equal(new TextRange(At(7, 1), At(7, 16)), location.Range);
    }

    [Fact]
    public async Task Hover_documents_commands()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);

        var hover = await _service.GetHoverAsync(TestEnvironment.ModIni, 4, 1);

        Assert.NotNull(hover);
        Assert.Contains("**run**", hover!.Markdown);
        Assert.Null(await _service.GetHoverAsync(TestEnvironment.ModIni, 40, 0));
    }

    [Fact]
    public async Task References_cover_every_file_of_the_mod()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);

        var references = await _service.FindReferencesAsync(TestEnvironment.ModIni, 8, 5); // [CommandListLocal]

        Assert.Equal(3, references.Count);
        Assert.Contains(references, r => r.Path == System.IO.Path.Combine(TestEnvironment.Mod, "extra.ini"));
    }

    [Fact]
    public async Task Completion_offers_command_lists_after_run()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);
        var text = System.IO.File.ReadAllText(TestEnvironment.ModIni).Replace("\r\n", "\n");
        await _service.UpdateDocumentAsync(TestEnvironment.ModIni, text + "\n[TextureOverrideMore]\nrun = \n");
        var line = text.Count(c => c == '\n') + 2;

        var items = await _service.GetCompletionsAsync(TestEnvironment.ModIni, line, 6);

        var labels = items.Select(i => i.Label).ToList();
        Assert.Contains("CommandListLocal", labels);
        Assert.Contains("CommandList\\Lib\\Draw", labels);
        Assert.All(items, i => Assert.False(string.IsNullOrEmpty(i.NewText)));
    }

    [Fact]
    public async Task Outline_groups_sections_by_kind()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);

        var outline = await _service.GetOutlineAsync(TestEnvironment.ModIni);

        Assert.Equal(
            new[] { "TextureOverride:TextureOverrideBody", "CommandList:CommandListLocal" },
            outline.Select(s => s.Name + ":" + string.Join(",", s.Children.Select(c => c.Name))));
        Assert.All(outline, s => Assert.Equal(SymbolKind.Namespace, s.Kind));
    }

    [Fact]
    public async Task Rename_updates_every_file_and_refuses_library_symbols()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);

        var check = await _service.PrepareRenameAsync(TestEnvironment.ModIni, 8, 14);
        Assert.True(check.CanRename);
        Assert.Equal("Local", check.Placeholder);

        var edit = await _service.RenameAsync(TestEnvironment.ModIni, 8, 14, "Shared");
        Assert.Equal(
            new[] { System.IO.Path.Combine(TestEnvironment.Mod, "extra.ini"), TestEnvironment.ModIni }.OrderBy(p => p),
            edit.Changes.Keys.OrderBy(p => p));

        var refused = await _service.PrepareRenameAsync(TestEnvironment.ModIni, 5, 20);
        Assert.False(refused.CanRename);
        Assert.Contains("declared in the package", refused.Message);
        await Assert.ThrowsAsync<XxmiRequestException>(() => _service.RenameAsync(TestEnvironment.ModIni, 5, 20, "Nope"));
    }

    [Fact]
    public async Task Requests_for_documents_that_are_not_open_say_so()
    {
        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.GetHoverAsync(TestEnvironment.ModIni, 0, 0));
    }

    [Fact]
    public async Task A_closed_document_is_forgotten()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);
        await _service.CloseDocumentAsync(TestEnvironment.ModIni);

        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.GetDiagnosticsAsync(TestEnvironment.ModIni));
    }

    [Fact]
    public async Task The_service_works_through_the_interface()
    {
        IXxmiLanguageService service = _service;

        await service.OpenDocumentAsync(TestEnvironment.ModIni);

        Assert.Empty(await service.GetDiagnosticsAsync(TestEnvironment.ModIni));
        Assert.Contains("**run**", (await service.GetHoverAsync(TestEnvironment.ModIni, 4, 1))!.Markdown);
        Assert.Empty(await service.GetCodeActionsAsync(TestEnvironment.ModIni, 4)); // the line overload
    }

    [Fact]
    public async Task Reports_the_server_version()
    {
        Assert.Matches(@"^\d+\.\d+\.\d+", _service.ServerVersion);
        Assert.True(_service.IsRunning);
        Assert.NotNull(_service.ServerProcessId);
        await Task.CompletedTask;
    }

    [Fact]
    public async Task A_killed_server_is_reported_fails_calls_and_can_be_restarted()
    {
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);
        Assert.Empty(await _service.GetDiagnosticsAsync(TestEnvironment.ModIni));
        var exited = new TaskCompletionSource<ServerExitedEventArgs>(TaskCreationOptions.RunContinuationsAsynchronously);
        _service.ServerExited += (_, e) => exited.TrySetResult(e);

        using (var process = Process.GetProcessById(_service.ServerProcessId!.Value)) process.Kill();

        var args = await exited.Task.WaitAsync(TimeSpan.FromSeconds(20));
        Assert.False(_service.IsRunning);
        Assert.NotNull(args);
        var error = await Assert.ThrowsAsync<XxmiServerExitedException>(() => _service.GetHoverAsync(TestEnvironment.ModIni, 4, 1));
        Assert.Contains("StartAsync", error.Message);

        // Restart: the open document comes back with its text.
        await _service.StartAsync(TestEnvironment.Mod);
        Assert.True(_service.IsRunning);
        Assert.Contains("**run**", (await _service.GetHoverAsync(TestEnvironment.ModIni, 4, 1))!.Markdown);
        Assert.Empty(await _service.GetDiagnosticsAsync(TestEnvironment.ModIni));
    }

    [Fact]
    public async Task StartAsync_while_running_is_refused_and_RestartAsync_works()
    {
        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.StartAsync(TestEnvironment.Mod));
        await _service.OpenDocumentAsync(TestEnvironment.ModIni);
        var before = _service.ServerProcessId;

        await _service.RestartAsync();

        Assert.NotEqual(before, _service.ServerProcessId);
        Assert.Empty(await _service.GetDiagnosticsAsync(TestEnvironment.ModIni));
    }

    [Fact]
    public async Task StopAsync_ends_the_process_without_ServerExited()
    {
        var raised = false;
        _service.ServerExited += (_, _) => raised = true;
        var pid = _service.ServerProcessId!.Value;

        await _service.StopAsync();
        await Task.Delay(300);

        Assert.False(_service.IsRunning);
        Assert.False(raised);
        Assert.Throws<ArgumentException>(() => Process.GetProcessById(pid)); // gone
    }

    [Fact]
    public async Task A_service_that_was_never_started_says_so()
    {
        await using var fresh = new XxmiLanguageService(TestEnvironment.Options());

        await Assert.ThrowsAsync<InvalidOperationException>(() => fresh.OpenDocumentAsync(TestEnvironment.ModIni));
    }

    [Fact]
    public async Task Disposed_services_throw()
    {
        var other = new XxmiLanguageService(TestEnvironment.Options());
        await other.DisposeAsync();

        await Assert.ThrowsAsync<ObjectDisposedException>(() => other.StartAsync());
    }
}
