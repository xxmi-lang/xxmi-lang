using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Nodes;
using Xunit;

namespace XxmiLang.Core.Tests;

public class JsonContextTests
{
    private static TextRange Range(int l1, int c1, int l2, int c2) => new(new TextPosition(l1, c1), new TextPosition(l2, c2));

    private static XxmiDiagnostic Diagnostic(bool withFix) =>
        new(
            "XM201",
            DiagnosticSeverity.Error,
            "Unresolved command list `CommandListLocl`.",
            Range(6, 6, 6, 21),
            withFix ? new QuickFix("Change to `CommandListLocal`", new[] { new TextEdit(Range(6, 6, 6, 21), "CommandListLocal") }) : null);

    /// <summary>Serialize, read back, serialize again: the two texts match (records don't compare their lists by value).</summary>
    private static string RoundTrip<T>(T value, System.Text.Json.Serialization.Metadata.JsonTypeInfo<T> info)
    {
        var first = JsonSerializer.Serialize(value, info);
        var back = JsonSerializer.Deserialize(first, info);
        Assert.NotNull(back);
        Assert.Equal(first, JsonSerializer.Serialize(back, info));
        return first;
    }

    [Fact]
    public void A_diagnostic_with_a_fix_round_trips_in_the_shape_of_the_CLI_json()
    {
        var json = RoundTrip(Diagnostic(withFix: true), XxmiJsonContext.Default.XxmiDiagnostic);

        // Compared as JSON: the default encoder writes a backtick as \u0060.
        Assert.True(JsonNode.DeepEquals(
            JsonNode.Parse("""{"id":"XM201","severity":"error","message":"Unresolved command list `CommandListLocl`.","range":{"start":{"line":6,"character":6},"end":{"line":6,"character":21}},"fix":{"title":"Change to `CommandListLocal`","edits":[{"range":{"start":{"line":6,"character":6},"end":{"line":6,"character":21}},"newText":"CommandListLocal"}]}}"""),
            JsonNode.Parse(json)));
    }

    [Fact]
    public void Nulls_are_left_out()
    {
        var json = RoundTrip(Diagnostic(withFix: false), XxmiJsonContext.Default.XxmiDiagnostic);

        Assert.DoesNotContain("fix", json);
        Assert.Null(JsonSerializer.Deserialize(json, XxmiJsonContext.Default.XxmiDiagnostic)!.Fix);
    }

    [Theory]
    [InlineData("error", DiagnosticSeverity.Error)]
    [InlineData("warning", DiagnosticSeverity.Warning)]
    [InlineData("info", DiagnosticSeverity.Info)]
    [InlineData("hint", DiagnosticSeverity.Hint)]
    public void Severity_is_a_lowercase_word(string word, DiagnosticSeverity expected)
    {
        var read = JsonSerializer.Deserialize($"\"{word}\"", XxmiJsonContext.Default.DiagnosticSeverity);

        Assert.Equal(expected, read);
        Assert.Equal($"\"{expected.ToString().ToLowerInvariant()}\"", JsonSerializer.Serialize(expected, XxmiJsonContext.Default.DiagnosticSeverity));
    }

    [Fact]
    public void An_unknown_severity_is_refused()
    {
        Assert.Throws<JsonException>(() => JsonSerializer.Deserialize("\"fatal\"", XxmiJsonContext.Default.DiagnosticSeverity));
    }

    [Fact]
    public void The_lint_json_of_the_CLI_reads_into_a_LintResult()
    {
        const string cli = """
            {
              "version": 1,
              "files": [
                { "path": "Mods/M/mod.ini", "encoding": "utf8",
                  "diagnostics": [ { "id": "XM203", "severity": "warning", "message": "m", "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 5 } } } ] },
                { "path": "Mods/M/b.ini", "encoding": "utf8bom", "diagnostics": [] }
              ],
              "summary": { "files": 2, "error": 0, "warning": 1, "info": 0, "hint": 0 }
            }
            """;

        var result = JsonSerializer.Deserialize(cli, XxmiJsonContext.Default.LintResult)!;

        Assert.Equal(new LintSummary(2, 0, 1, 0, 0), result.Summary);
        Assert.Equal(new[] { "Mods/M/mod.ini", "Mods/M/b.ini" }, result.Files.Select(f => f.Path));
        Assert.Equal("utf8bom", result.Files[1].Encoding);
        var (path, d) = Assert.Single(result.AllDiagnostics);
        Assert.Equal("Mods/M/mod.ini", path);
        Assert.Equal(DiagnosticSeverity.Warning, d.Severity);
        Assert.Equal(Range(1, 2, 1, 5), d.Range);
        Assert.False(result.HasErrors);
        Assert.Equal(1, result.WarningCount);
    }

    [Fact]
    public void A_LintResult_writes_only_its_data_not_the_helpers()
    {
        var result = new LintResult(
            new[] { new LintFile("/m/mod.ini", "utf8", new[] { Diagnostic(withFix: true) }) },
            new LintSummary(1, 1, 0, 0, 0));

        var json = RoundTrip(result, XxmiJsonContext.Default.LintResult);

        var node = JsonNode.Parse(json)!.AsObject();
        Assert.Equal(new[] { "files", "summary" }, node.Select(p => p.Key).OrderBy(k => k));
        Assert.Equal(1, node["summary"]!["error"]!.GetValue<int>());
        Assert.Equal("/m/mod.ini", node["files"]![0]!["path"]!.GetValue<string>());
        var back = JsonSerializer.Deserialize(json, XxmiJsonContext.Default.LintResult)!;
        Assert.True(back.HasErrors);
        Assert.Equal("XM201", Assert.Single(back.AllDiagnostics).Diagnostic.Id);
    }

    [Fact]
    public void Locations_hovers_and_edits_round_trip()
    {
        RoundTrip(new XxmiLocation("/lib.ini", "file:///lib.ini", Range(7, 1, 7, 16)), XxmiJsonContext.Default.XxmiLocation);
        var hover = RoundTrip(new HoverResult("**run**", Range(4, 0, 4, 3)), XxmiJsonContext.Default.HoverResult);
        Assert.Contains("\"markdown\":\"**run**\"", hover);
        RoundTrip(new HoverResult("no range", null), XxmiJsonContext.Default.HoverResult);
        RoundTrip(new RenameCheck(false, null, null, "declared in the package"), XxmiJsonContext.Default.RenameCheck);
        RoundTrip(new RenameCheck(true, Range(8, 13, 8, 18), "Local", null), XxmiJsonContext.Default.RenameCheck);
        RoundTrip<IReadOnlyList<TextEdit>>(new[] { new TextEdit(Range(0, 0, 0, 1), "é✓") }, XxmiJsonContext.Default.IReadOnlyListTextEdit);
    }

    [Fact]
    public void Kinds_are_numbers_and_the_outline_is_recursive()
    {
        var outline = new OutlineSymbol(
            "CommandList",
            SymbolKind.Namespace,
            Range(0, 0, 4, 0),
            Range(0, 0, 0, 11),
            new[] { new OutlineSymbol("CommandListLocal", SymbolKind.Function, Range(0, 0, 2, 0), Range(0, 1, 0, 17), Array.Empty<OutlineSymbol>()) });

        var json = RoundTrip(outline, XxmiJsonContext.Default.OutlineSymbol);

        Assert.Contains("\"kind\":3", json);
        Assert.Contains("\"kind\":12", json);
        var back = JsonSerializer.Deserialize(json, XxmiJsonContext.Default.OutlineSymbol)!;
        Assert.Equal(SymbolKind.Function, Assert.Single(back.Children).Kind);

        var item = new CompletionItem("CommandListLocal", CompletionItemKind.Reference, null, "docs", "CommandListLocal", Range(5, 6, 5, 6));
        Assert.Contains("\"kind\":18", RoundTrip(item, XxmiJsonContext.Default.CompletionItem));
    }

    [Fact]
    public void A_workspace_edit_and_a_code_action_round_trip()
    {
        var edit = new WorkspaceEdit(new Dictionary<string, IReadOnlyList<TextEdit>>
        {
            ["/m/mod.ini"] = new[] { new TextEdit(Range(8, 13, 8, 18), "Shared") },
            ["/m/extra.ini"] = new[] { new TextEdit(Range(2, 6, 2, 11), "Shared"), new TextEdit(Range(3, 6, 3, 11), "Shared") },
        });

        RoundTrip(edit, XxmiJsonContext.Default.WorkspaceEdit);
        var back = JsonSerializer.Deserialize(JsonSerializer.Serialize(edit, XxmiJsonContext.Default.WorkspaceEdit), XxmiJsonContext.Default.WorkspaceEdit)!;
        Assert.Equal(2, back.Changes["/m/extra.ini"].Count);
        var action = new XxmiCodeAction("Change to `X`", "quickfix", true, edit);
        RoundTrip(action, XxmiJsonContext.Default.XxmiCodeAction);
        RoundTrip(new XxmiCodeAction("No edit", null, false, null), XxmiJsonContext.Default.XxmiCodeAction);
    }

    [Fact]
    public void Lists_have_their_own_metadata()
    {
        IReadOnlyList<XxmiDiagnostic> list = new[] { Diagnostic(true), Diagnostic(false) };

        var json = RoundTrip(list, XxmiJsonContext.Default.IReadOnlyListXxmiDiagnostic);

        Assert.Equal(2, JsonNode.Parse(json)!.AsArray().Count);
        RoundTrip<IReadOnlyList<XxmiLocation>>(Array.Empty<XxmiLocation>(), XxmiJsonContext.Default.IReadOnlyListXxmiLocation);
    }

    [Fact]
    public void The_context_does_not_depend_on_reflection()
    {
        // A type that isn't registered has no metadata: the context says so instead of falling back to reflection.
        Assert.Null(XxmiJsonContext.Default.GetTypeInfo(typeof(Version)));
        Assert.NotNull(XxmiJsonContext.Default.GetTypeInfo(typeof(LintResult)));
    }
}
