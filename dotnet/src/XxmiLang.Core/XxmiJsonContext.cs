using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace XxmiLang
{
    /// <summary>
    /// Source-generated System.Text.Json metadata for the result types (trim and Native AOT safe; no reflection).
    /// camelCase names, <c>null</c> left out, <see cref="DiagnosticSeverity"/> as <c>"error"</c> / <c>"warning"</c> / …,
    /// the other enums as numbers. The diagnostics, fixes, ranges and summary have the shape of <c>xxmi lint --format json</c>.
    /// </summary>
    /// <example><c>JsonSerializer.Serialize(result, XxmiJsonContext.Default.LintResult)</c></example>
    [JsonSourceGenerationOptions(
        PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase,
        PropertyNameCaseInsensitive = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull)]
    [JsonSerializable(typeof(TextPosition))]
    [JsonSerializable(typeof(TextRange))]
    [JsonSerializable(typeof(TextEdit))]
    [JsonSerializable(typeof(QuickFix))]
    [JsonSerializable(typeof(DiagnosticSeverity))]
    [JsonSerializable(typeof(XxmiDiagnostic))]
    [JsonSerializable(typeof(LintFile))]
    [JsonSerializable(typeof(LintSummary))]
    [JsonSerializable(typeof(LintResult))]
    [JsonSerializable(typeof(XxmiLocation))]
    [JsonSerializable(typeof(HoverResult))]
    [JsonSerializable(typeof(CompletionItem))]
    [JsonSerializable(typeof(OutlineSymbol))]
    [JsonSerializable(typeof(WorkspaceEdit))]
    [JsonSerializable(typeof(RenameCheck))]
    [JsonSerializable(typeof(XxmiCodeAction))]
    [JsonSerializable(typeof(IReadOnlyList<TextEdit>))]
    [JsonSerializable(typeof(IReadOnlyList<XxmiDiagnostic>))]
    [JsonSerializable(typeof(IReadOnlyList<XxmiLocation>))]
    [JsonSerializable(typeof(IReadOnlyList<CompletionItem>))]
    [JsonSerializable(typeof(IReadOnlyList<OutlineSymbol>))]
    [JsonSerializable(typeof(IReadOnlyList<XxmiCodeAction>))]
    public partial class XxmiJsonContext : JsonSerializerContext
    {
    }
}
