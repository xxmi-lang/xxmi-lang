using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Xunit;

namespace XxmiLang.Tests;

public class LinterTests
{
    private const string TypoMod = "[CommandListLocal]\nhandling = skip\n\n[TextureOverrideBody]\nhash = 1a2b3c4d\nrun = CommandListLocal\nrun = CommandListLocl\n";

    [Fact]
    public async Task A_clean_mod_has_no_diagnostics()
    {
        var result = await XxmiLinter.LintAsync(TestEnvironment.Mod, TestEnvironment.LintOptions());

        Assert.False(result.HasErrors);
        Assert.Equal(2, result.Files.Count); // mod.ini and extra.ini
        Assert.All(result.Files, f => Assert.Empty(f.Diagnostics));
        Assert.Contains(result.Files, f => f.Path == TestEnvironment.ModIni);
        Assert.Equal(new LintSummary(2, 0, 0, 0, 0), result.Summary);
    }

    [Fact]
    public async Task An_unresolved_run_target_is_XM201_and_findings_are_not_an_exception()
    {
        var directory = TestEnvironment.TempMod(TypoMod);
        try
        {
            var result = await XxmiLinter.LintAsync(new[] { directory }, TestEnvironment.LintOptions());

            Assert.True(result.HasErrors);
            var (path, diagnostic) = Assert.Single(result.AllDiagnostics);
            Assert.Equal(Path.Combine(directory, "mod.ini"), path);
            Assert.Equal("XM201", diagnostic.Id);
            Assert.Equal(DiagnosticSeverity.Error, diagnostic.Severity);
            Assert.Contains("CommandListLocl", diagnostic.Message);
            Assert.Equal(new TextRange(new TextPosition(6, 6), new TextPosition(6, 21)), diagnostic.Range);
            Assert.Equal(1, result.ErrorCount);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task The_quick_fix_can_be_applied()
    {
        var directory = TestEnvironment.TempMod(TypoMod);
        try
        {
            var result = await XxmiLinter.LintAsync(directory, TestEnvironment.LintOptions());

            var fix = Assert.Single(result.AllDiagnostics).Diagnostic.Fix;
            Assert.NotNull(fix);
            Assert.Equal("Change to `CommandListLocal`", fix!.Title);
            Assert.Equal(TypoMod.Replace("CommandListLocl", "CommandListLocal"), TextEdits.Apply(TypoMod, fix.Edits));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task Rules_limits_what_runs()
    {
        var directory = TestEnvironment.TempMod(TypoMod);
        try
        {
            var options = TestEnvironment.LintOptions();
            options.Rules = new[] { "XM202" };

            Assert.Empty((await XxmiLinter.LintAsync(directory, options)).AllDiagnostics);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task Relative_paths_are_resolved_against_WorkingDirectory()
    {
        var options = TestEnvironment.LintOptions();
        options.WorkingDirectory = TestEnvironment.Package;

        var result = await XxmiLinter.LintAsync(new[] { Path.Combine("Mods", "MyMod") }, options);

        Assert.Contains(result.Files, f => f.Path == TestEnvironment.ModIni);
    }

    [Fact]
    public async Task A_missing_path_is_an_error_with_the_CLI_message()
    {
        var missing = Path.Combine(Path.GetTempPath(), "xxmi-no-such-" + Guid.NewGuid().ToString("N"));

        var error = await Assert.ThrowsAsync<XxmiProcessException>(() => XxmiLinter.LintAsync(missing, TestEnvironment.LintOptions()));

        Assert.Equal(2, error.ExitCode);
        Assert.Contains("no such file or directory", error.StandardError);
        Assert.Contains("exit code 2", error.Message);
    }

    [Fact]
    public async Task A_missing_executable_lists_what_was_tried()
    {
        var options = new LintOptions { ExecutablePath = Path.Combine(Path.GetTempPath(), "no-such-xxmi") };

        var error = await Assert.ThrowsAsync<XxmiBinaryNotFoundException>(() => XxmiLinter.LintAsync(TestEnvironment.Mod, options));

        Assert.Contains("no-such-xxmi", error.Message);
    }

    [Fact]
    public async Task Cancellation_is_honoured()
    {
        using var cts = new System.Threading.CancellationTokenSource();
        cts.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => XxmiLinter.LintAsync(TestEnvironment.Mod, TestEnvironment.LintOptions(), cts.Token));
    }

    [Fact]
    public async Task Many_paths_are_split_over_several_runs_and_merged()
    {
        var directory = TestEnvironment.TempMod(TypoMod);
        try
        {
            // Far more than fits on one command line (32 KB on Windows): the linter has to split the paths.
            var file = Path.Combine(directory, "mod.ini");
            var paths = Enumerable.Repeat(file, 2000).ToList();

            var result = await XxmiLinter.LintAsync(paths, TestEnvironment.LintOptions());

            Assert.Equal(file, Assert.Single(result.Files).Path); // a file linted in several batches is listed once
            Assert.Equal(1, result.ErrorCount);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
}
