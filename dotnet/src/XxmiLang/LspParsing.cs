using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

namespace XxmiLang
{
    /// <summary>Reads LSP JSON into the public types. Tolerant: unknown or missing optional parts are skipped.</summary>
    internal static class LspParsing
    {
        public static TextPosition Position(JsonElement e) => new TextPosition(Int(e, "line"), Int(e, "character"));

        public static TextRange Range(JsonElement e) => new TextRange(Position(e.GetProperty("start")), Position(e.GetProperty("end")));

        public static TextRange? OptionalRange(JsonElement e) =>
            e.ValueKind == JsonValueKind.Object && e.TryGetProperty("start", out _) ? Range(e) : (TextRange?)null;

        public static TextEdit Edit(JsonElement e) => new TextEdit(Range(e.GetProperty("range")), Str(e, "newText") ?? string.Empty);

        public static IReadOnlyList<TextEdit> Edits(JsonElement e) =>
            e.ValueKind == JsonValueKind.Array ? e.EnumerateArray().Select(Edit).ToList() : new List<TextEdit>();

        public static QuickFix? Fix(JsonElement data)
        {
            if (data.ValueKind != JsonValueKind.Object || !data.TryGetProperty("edits", out var edits)) return null;
            return new QuickFix(Str(data, "title") ?? string.Empty, Edits(edits));
        }

        public static XxmiDiagnostic Diagnostic(JsonElement e)
        {
            var id = string.Empty;
            if (e.TryGetProperty("code", out var code))
            {
                id = code.ValueKind == JsonValueKind.String ? code.GetString()! : code.ValueKind == JsonValueKind.Number ? code.GetRawText() : string.Empty;
            }
            var severity = e.TryGetProperty("severity", out var s) && s.ValueKind == JsonValueKind.Number ? (DiagnosticSeverity)s.GetInt32() : DiagnosticSeverity.Error;
            e.TryGetProperty("data", out var data);
            return new XxmiDiagnostic(id, severity, Str(e, "message") ?? string.Empty, Range(e.GetProperty("range")), Fix(data));
        }

        public static XxmiLocation? Location(JsonElement e)
        {
            if (e.ValueKind == JsonValueKind.Array) return e.GetArrayLength() == 0 ? null : Location(e[0]);
            if (e.ValueKind != JsonValueKind.Object) return null;
            // Location, or LocationLink (targetUri / targetSelectionRange).
            var uri = Str(e, "uri") ?? Str(e, "targetUri");
            var range = e.TryGetProperty("range", out var r) ? r : e.TryGetProperty("targetSelectionRange", out var t) ? t : default;
            if (uri is null || range.ValueKind != JsonValueKind.Object) return null;
            return new XxmiLocation(FileUris.ToPath(uri), uri, Range(range));
        }

        public static HoverResult? Hover(JsonElement e)
        {
            if (e.ValueKind != JsonValueKind.Object || !e.TryGetProperty("contents", out var contents)) return null;
            var text = Markup(contents);
            if (text is null) return null;
            return new HoverResult(text, e.TryGetProperty("range", out var range) ? OptionalRange(range) : null);
        }

        // MarkupContent, a string, a MarkedString ({language, value}) or an array of those.
        public static string? Markup(JsonElement e)
        {
            switch (e.ValueKind)
            {
                case JsonValueKind.String: return e.GetString();
                case JsonValueKind.Object: return Str(e, "value");
                case JsonValueKind.Array: return string.Join("\n\n", e.EnumerateArray().Select(Markup).Where(s => !string.IsNullOrEmpty(s)));
                default: return null;
            }
        }

        public static CompletionItem Completion(JsonElement e)
        {
            var label = Str(e, "label") ?? string.Empty;
            var newText = Str(e, "insertText") ?? label;
            TextRange? range = null;
            if (e.TryGetProperty("textEdit", out var edit) && edit.ValueKind == JsonValueKind.Object)
            {
                newText = Str(edit, "newText") ?? newText;
                // TextEdit has "range"; InsertReplaceEdit has "replace"/"insert".
                if (edit.TryGetProperty("range", out var r)) range = OptionalRange(r);
                else if (edit.TryGetProperty("replace", out var replace)) range = OptionalRange(replace);
            }
            var kind = e.TryGetProperty("kind", out var k) && k.ValueKind == JsonValueKind.Number ? (CompletionItemKind)k.GetInt32() : CompletionItemKind.Text;
            var documentation = e.TryGetProperty("documentation", out var doc) ? Markup(doc) : null;
            return new CompletionItem(label, kind, Str(e, "detail"), documentation, newText, range);
        }

        public static OutlineSymbol Symbol(JsonElement e)
        {
            var children = e.TryGetProperty("children", out var c) && c.ValueKind == JsonValueKind.Array
                ? c.EnumerateArray().Select(Symbol).ToList()
                : new List<OutlineSymbol>();
            var range = Range(e.GetProperty("range"));
            var selection = e.TryGetProperty("selectionRange", out var s) ? Range(s) : range;
            var kind = e.TryGetProperty("kind", out var k) && k.ValueKind == JsonValueKind.Number ? (SymbolKind)k.GetInt32() : SymbolKind.Object;
            return new OutlineSymbol(Str(e, "name") ?? string.Empty, kind, range, selection, children);
        }

        public static WorkspaceEdit? WorkspaceEdit(JsonElement e)
        {
            if (e.ValueKind != JsonValueKind.Object) return null;
            var changes = new Dictionary<string, IReadOnlyList<TextEdit>>(StringComparer.Ordinal);
            if (e.TryGetProperty("changes", out var c) && c.ValueKind == JsonValueKind.Object)
            {
                foreach (var entry in c.EnumerateObject()) changes[FileUris.ToPath(entry.Name)] = Edits(entry.Value);
            }
            return new WorkspaceEdit(changes);
        }

        public static XxmiCodeAction CodeAction(JsonElement e) =>
            new XxmiCodeAction(
                Str(e, "title") ?? string.Empty,
                Str(e, "kind"),
                e.TryGetProperty("isPreferred", out var p) && p.ValueKind == JsonValueKind.True,
                e.TryGetProperty("edit", out var edit) ? WorkspaceEdit(edit) : null);

        public static string? Str(JsonElement e, string name) =>
            e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;

        private static int Int(JsonElement e, string name) => e.GetProperty(name).GetInt32();
    }
}
