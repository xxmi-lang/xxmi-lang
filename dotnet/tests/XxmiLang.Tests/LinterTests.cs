using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Xunit;

namespace XxmiLang.Tests;

public class LinterTests
{
    private static readonly XxmiLinter Linter = TestEnvironment.Linter();
    private static readonly LintOptions XM201Only = new() { Rules = new[] { "XM201" } };
    private static readonly LintOptions XM201AndXM206 = new() { Rules = new[] { "XM201", "XM206" } };

    private const string TypoMod = "[CommandListLocal]\nhandling = skip\n\n[TextureOverrideBody]\nhash = 1a2b3c4d\nrun = CommandListLocal\nrun = CommandListLocl\n";

    [Fact]
    public async Task A_clean_mod_has_no_diagnostics()
    {
        var result = await Linter.LintAsync(new[] { TestEnvironment.Mod });

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
            var result = await Linter.LintAsync(new[] { directory });

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
            var result = await Linter.LintAsync(new[] { directory });

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
            var options = new LintOptions { Rules = new[] { "XM202" } };

            Assert.Empty((await Linter.LintAsync(new[] { directory }, options)).AllDiagnostics);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task Relative_paths_are_resolved_against_WorkingDirectory()
    {
        var linter = TestEnvironment.Linter(TestEnvironment.Package);

        var result = await linter.LintAsync(new[] { Path.Combine("Mods", "MyMod") });

        Assert.Contains(result.Files, f => f.Path == TestEnvironment.ModIni);
    }

    [Fact]
    public async Task A_missing_path_is_an_error_with_the_CLI_message()
    {
        var missing = Path.Combine(Path.GetTempPath(), "xxmi-no-such-" + Guid.NewGuid().ToString("N"));

        var error = await Assert.ThrowsAsync<XxmiProcessException>(() => Linter.LintAsync(new[] { missing }));

        Assert.Equal(2, error.ExitCode);
        Assert.Contains("no such file or directory", error.StandardError);
        Assert.Contains("exit code 2", error.Message);
    }

    [Fact]
    public async Task A_missing_executable_lists_what_was_tried()
    {
        var linter = new XxmiLinter(new XxmiOptions { ExecutablePath = Path.Combine(Path.GetTempPath(), "no-such-xxmi") });

        var error = await Assert.ThrowsAsync<XxmiBinaryNotFoundException>(() => linter.LintAsync(new[] { TestEnvironment.Mod }));

        Assert.Contains("no-such-xxmi", error.Message);
    }

    [Fact]
    public async Task Cancellation_is_honoured()
    {
        using var cts = new System.Threading.CancellationTokenSource();
        cts.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => Linter.LintAsync(new[] { TestEnvironment.Mod }, null, cts.Token));
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

            var result = await Linter.LintAsync(paths);

            Assert.Equal(file, Assert.Single(result.Files).Path); // a file linted in several batches is listed once
            Assert.Equal(1, result.ErrorCount);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task LintFileAsync_lints_one_file_and_refuses_a_folder()
    {
        var directory = TestEnvironment.TempMod(TypoMod);
        try
        {
            var file = Path.Combine(directory, "mod.ini");

            var result = await Linter.LintFileAsync(file);

            Assert.Equal(file, Assert.Single(result.Files).Path);
            Assert.Equal("XM201", Assert.Single(result.AllDiagnostics).Diagnostic.Id);
            Assert.True(result.HasErrors);
            await Assert.ThrowsAsync<ArgumentException>(() => Linter.LintFileAsync(directory));
            // Relative paths use the working directory, like the batch call.
            var relative = await TestEnvironment.Linter(directory).LintFileAsync("mod.ini");
            Assert.Equal(file, Assert.Single(relative.Files).Path);
            // A file that isn't there is the CLI's usage error.
            var missing = await Assert.ThrowsAsync<XxmiProcessException>(() => Linter.LintFileAsync(Path.Combine(directory, "gone.ini")));
            Assert.Equal(2, missing.ExitCode);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task LintTextAsync_lints_the_text_instead_of_the_file_on_disk()
    {
        var directory = TestEnvironment.TempMod("[CommandListLocal]\nhandling = skip\n"); // clean on disk
        try
        {
            var file = Path.Combine(directory, "mod.ini");

            var result = await Linter.LintTextAsync(file, TypoMod, XM201Only);

            Assert.Equal(file, Assert.Single(result.Files).Path);
            var diagnostic = Assert.Single(result.AllDiagnostics).Diagnostic;
            Assert.Equal("XM201", diagnostic.Id);
            Assert.Equal(new TextRange(new TextPosition(6, 6), new TextPosition(6, 21)), diagnostic.Range);
            Assert.Equal(new LintSummary(1, 1, 0, 0, 0), result.Summary);
            Assert.Equal(TypoMod.Replace("CommandListLocl", "CommandListLocal"), TextEdits.Apply(TypoMod, diagnostic.Fix!.Edits));
            // And the same file from disk is clean: the text really is what was linted.
            Assert.Empty((await Linter.LintFileAsync(file, XM201Only)).AllDiagnostics);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task LintTextAsync_does_not_need_the_file_to_exist_and_agrees_with_LintFileAsync()
    {
        var path = Path.Combine(TestEnvironment.Mod, "unsaved.ini");
        Assert.False(File.Exists(path));
        var onDisk = await Linter.LintFileAsync(TestEnvironment.ModIni);

        var clean = await Linter.LintTextAsync(path, File.ReadAllText(TestEnvironment.ModIni));
        var same = await Linter.LintTextAsync(TestEnvironment.ModIni, File.ReadAllText(TestEnvironment.ModIni));

        Assert.Equal(path, Assert.Single(clean.Files).Path);
        Assert.Equal(onDisk.Files[0].Diagnostics, same.Files[0].Diagnostics);
    }

    [Fact]
    public async Task LintTextAsync_takes_the_namespace_from_the_path()
    {
        // The text names itself by its path-derived namespace. Inside the package (d3dx.ini at the root) that is
        // Mods\Other\new.ini; at a path outside any package it is just new.ini, and the reference doesn't resolve.
        var root = Path.Combine(Path.GetTempPath(), "xxmi-lang-ns-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(Path.Combine(root, "package", "Mods", "Other"));
        Directory.CreateDirectory(Path.Combine(root, "loose"));
        File.WriteAllText(Path.Combine(root, "package", "d3dx.ini"), "[Include]\ninclude_recursive = Mods\n");
        try
        {
            const string text = "[CommandListFoo]\n\n[CommandListCaller]\nrun = CommandList\\Mods\\Other\\new.ini\\Foo\n";

            var inPackage = await Linter.LintTextAsync(Path.Combine(root, "package", "Mods", "Other", "new.ini"), text, XM201AndXM206);
            var outside = await Linter.LintTextAsync(Path.Combine(root, "loose", "new.ini"), text, XM201AndXM206);

            Assert.Empty(inPackage.AllDiagnostics);
            var diagnostic = Assert.Single(outside.AllDiagnostics).Diagnostic;
            Assert.Equal("XM206", diagnostic.Id);
            Assert.Contains("Mods\\Other\\new.ini", diagnostic.Message);
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    [Fact]
    public async Task LintTextAsync_handles_unicode_and_large_text()
    {
        var path = Path.Combine(TestEnvironment.Mod, "big.ini");
        // ~1.5 MB: larger than a pipe buffer, with non-ASCII comments; a CN mod is full of those.
        var text = "; 角色模组 é ✓\n[CommandListLocal]\nhandling = skip\n" +
                   string.Concat(Enumerable.Range(0, 30_000).Select(i => $"; 注释 {i} ✓ ................................................\n")) +
                   "[TextureOverrideBody]\nhash = 1a2b3c4d\nrun = CommandListLocl\n";

        var result = await Linter.LintTextAsync(path, text, new LintOptions { Rules = new[] { "XM201" } });

        var diagnostic = Assert.Single(result.AllDiagnostics).Diagnostic;
        Assert.Equal("XM201", diagnostic.Id);
        Assert.Equal(30_005, diagnostic.Range.Start.Line);
    }

    [Fact]
    public void The_linter_is_an_IXxmiLinter()
    {
        IXxmiLinter linter = Linter;

        Assert.NotNull(linter);
    }
}
