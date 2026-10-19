using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Xunit;

namespace XxmiLang.Core.Tests;

/// <summary>The interfaces can be implemented without the XxmiLang package: a fake linter and a fake language service.</summary>
public class InterfaceTests
{
    private sealed class FakeLinter : IXxmiLinter
    {
        public List<string> Calls { get; } = new();

        private static LintResult Result(string path, params XxmiDiagnostic[] diagnostics) =>
            new(
                new[] { new LintFile(path, "utf8", diagnostics) },
                new LintSummary(1, diagnostics.Count(d => d.Severity == DiagnosticSeverity.Error), 0, 0, 0));

        public Task<LintResult> LintAsync(IEnumerable<string> paths, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            Calls.Add("batch:" + string.Join("|", paths));
            return Task.FromResult(new LintResult(paths.Select(p => new LintFile(p, "utf8", Array.Empty<XxmiDiagnostic>())).ToList(), new LintSummary(paths.Count(), 0, 0, 0, 0)));
        }

        public Task<LintResult> LintFileAsync(string path, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            Calls.Add("file:" + path);
            return Task.FromResult(Result(path));
        }

        public Task<LintResult> LintTextAsync(string path, string text, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            Calls.Add("text:" + path);
            var range = new TextRange(new TextPosition(0, 0), new TextPosition(0, text.Length));
            return Task.FromResult(Result(path, new XxmiDiagnostic("XM999", DiagnosticSeverity.Error, "fake", range)));
        }
    }

    // What code under test would do with a linter handed to it.
    private static async Task<bool> NeedsAttention(IXxmiLinter linter, string path, string unsavedText)
    {
        var result = await linter.LintTextAsync(path, unsavedText);
        return result.HasErrors;
    }

    [Fact]
    public async Task A_linter_can_be_faked()
    {
        var fake = new FakeLinter();

        Assert.True(await NeedsAttention(fake, "/m/mod.ini", "run = x"));
        Assert.Equal(0, (await fake.LintAsync(Array.Empty<string>())).Summary.Files);
        Assert.Equal(new[] { "text:/m/mod.ini", "batch:" }, fake.Calls);
    }

    private sealed class FakeLanguageService : IXxmiLanguageService
    {
        public TextRange? CodeActionRange { get; private set; }

        public event EventHandler<DiagnosticsPublishedEventArgs>? DiagnosticsPublished;

        public event EventHandler<ServerExitedEventArgs>? ServerExited;

        public event EventHandler<ServerLogEventArgs>? LogMessage;

        public bool IsRunning { get; private set; }

        public string? ServerVersion => "0.0.0-fake";

        public Task StartAsync(string? workspaceRoot = null, CancellationToken cancellationToken = default)
        {
            IsRunning = true;
            LogMessage?.Invoke(this, new ServerLogEventArgs("started", false));
            return Task.CompletedTask;
        }

        public Task RestartAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default)
        {
            IsRunning = false;
            return Task.CompletedTask;
        }

        public Task OpenDocumentAsync(string path, string? text = null, CancellationToken cancellationToken = default)
        {
            DiagnosticsPublished?.Invoke(this, new DiagnosticsPublishedEventArgs(path, Array.Empty<XxmiDiagnostic>()));
            return Task.CompletedTask;
        }

        public Task UpdateDocumentAsync(string path, string text, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task CloseDocumentAsync(string path, CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<IReadOnlyList<XxmiDiagnostic>> GetDiagnosticsAsync(string path, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<XxmiDiagnostic>>(Array.Empty<XxmiDiagnostic>());

        public Task<HoverResult?> GetHoverAsync(string path, int line, int character, CancellationToken cancellationToken = default) =>
            Task.FromResult<HoverResult?>(new HoverResult($"{line}:{character}", null));

        public Task<XxmiLocation?> GetDefinitionAsync(string path, int line, int character, CancellationToken cancellationToken = default) =>
            Task.FromResult<XxmiLocation?>(null);

        public Task<IReadOnlyList<XxmiLocation>> FindReferencesAsync(string path, int line, int character, bool includeDeclaration = true, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<XxmiLocation>>(Array.Empty<XxmiLocation>());

        public Task<IReadOnlyList<CompletionItem>> GetCompletionsAsync(string path, int line, int character, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<CompletionItem>>(Array.Empty<CompletionItem>());

        public Task<IReadOnlyList<OutlineSymbol>> GetOutlineAsync(string path, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<OutlineSymbol>>(Array.Empty<OutlineSymbol>());

        public Task<RenameCheck> PrepareRenameAsync(string path, int line, int character, CancellationToken cancellationToken = default) =>
            Task.FromResult(new RenameCheck(false, null, null, "fake"));

        public Task<WorkspaceEdit> RenameAsync(string path, int line, int character, string newName, CancellationToken cancellationToken = default) =>
            Task.FromResult(new WorkspaceEdit(new Dictionary<string, IReadOnlyList<TextEdit>>()));

        public Task<IReadOnlyList<XxmiCodeAction>> GetCodeActionsAsync(string path, TextRange range, CancellationToken cancellationToken = default)
        {
            CodeActionRange = range;
            return Task.FromResult<IReadOnlyList<XxmiCodeAction>>(Array.Empty<XxmiCodeAction>());
        }

        public ValueTask DisposeAsync() => ValueTask.CompletedTask;

        public void RaiseExited() => ServerExited?.Invoke(this, new ServerExitedEventArgs(1, "boom"));
    }

    [Fact]
    public async Task A_language_service_can_be_faked_and_the_events_are_usable()
    {
        await using var fake = new FakeLanguageService();
        IXxmiLanguageService service = fake;
        var logs = new List<string>();
        var published = new List<string>();
        var exits = new List<int?>();
        service.LogMessage += (_, e) => logs.Add(e.Message);
        service.DiagnosticsPublished += (_, e) => published.Add(e.Path);
        service.ServerExited += (_, e) => exits.Add(e.ExitCode);

        await service.StartAsync("/m");
        await service.OpenDocumentAsync("/m/mod.ini");
        fake.RaiseExited();

        Assert.True(service.IsRunning);
        Assert.Equal("1:2", (await service.GetHoverAsync("/m/mod.ini", 1, 2))!.Markdown);
        Assert.Equal(new[] { "started" }, logs);
        Assert.Equal(new[] { "/m/mod.ini" }, published);
        Assert.Equal(new int?[] { 1 }, exits);
    }

    [Fact]
    public async Task The_line_overload_of_GetCodeActionsAsync_is_a_default_member_over_the_range_one()
    {
        var fake = new FakeLanguageService();
        IXxmiLanguageService service = fake;

        await service.GetCodeActionsAsync("/m/mod.ini", 4);

        Assert.Equal(new TextRange(new TextPosition(4, 0), new TextPosition(4, int.MaxValue)), fake.CodeActionRange);
    }
}
