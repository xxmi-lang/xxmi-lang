using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using XxmiLang;

// dotnet run --project dotnet/samples/XxmiLang.Sample -- lint path/to/mod
// dotnet run --project dotnet/samples/XxmiLang.Sample -- hover path/to/mod.ini 5 2
// dotnet run --project dotnet/samples/XxmiLang.Sample -- text path/to/mod.ini      (lint the file's text as an in-memory buffer)
//
// Also publishes as a Native AOT executable: dotnet publish dotnet/samples/XxmiLang.Sample -c Release -r <rid>
//
// The xxmi binary comes from XXMI_PATH, a runtime package or PATH (see XxmiBinaryLocator).

if (args.Length >= 2 && args[0] == "lint")
{
    return await LintAsync(args.Skip(1));
}
if (args.Length == 4 && args[0] == "hover" && int.TryParse(args[2], out var line) && int.TryParse(args[3], out var column))
{
    return await HoverAsync(args[1], line, column);
}

if (args.Length == 2 && args[0] == "text")
{
    return await TextAsync(args[1]);
}

Console.Error.WriteLine("Usage:");
Console.Error.WriteLine("  lint <file-or-folder>...      print every diagnostic");
Console.Error.WriteLine("  hover <file.ini> <line> <col> open a file in the language server and print the hover (1-based, as editors show)");
Console.Error.WriteLine("  text <file.ini>               lint the contents of a file as text, as if it were that file (what an editor buffer would be)");
return 2;

static async Task<int> LintAsync(System.Collections.Generic.IEnumerable<string> paths)
{
    IXxmiLinter linter = new XxmiLinter();
    return Print(await linter.LintAsync(paths));
}

static async Task<int> TextAsync(string file)
{
    IXxmiLinter linter = new XxmiLinter();
    var path = Path.GetFullPath(file);
    return Print(await linter.LintTextAsync(path, await File.ReadAllTextAsync(path)));
}

static int Print(LintResult result)
{
    foreach (var (path, d) in result.AllDiagnostics)
    {
        var fix = d.Fix is null ? string.Empty : $"  [fix: {d.Fix.Title}]";
        Console.WriteLine($"{path}:{d.Range.Start.Line + 1}:{d.Range.Start.Character + 1}  {d.Severity.ToString().ToLowerInvariant()}  {d.Id}  {d.Message}{fix}");
    }
    var s = result.Summary;
    Console.WriteLine($"{s.Files} files: {s.Error} errors, {s.Warning} warnings, {s.Info} info, {s.Hint} hints");
    return result.HasErrors ? 1 : 0;
}

static async Task<int> HoverAsync(string file, int line, int column)
{
    var path = Path.GetFullPath(file);
    // Pass the mod folder, or the XXMI install above it, so references into the libraries resolve.
    await using IXxmiLanguageService service = new XxmiLanguageService();
    await service.StartAsync(Path.GetDirectoryName(path));
    Console.WriteLine($"xxmi language server {service.ServerVersion}");

    await service.OpenDocumentAsync(path);
    foreach (var d in await service.GetDiagnosticsAsync(path))
    {
        Console.WriteLine($"{d.Id}  line {d.Range.Start.Line + 1}: {d.Message}");
    }

    var hover = await service.GetHoverAsync(path, line - 1, column - 1);
    Console.WriteLine(hover is null ? $"Nothing to show at {line}:{column}." : hover.Markdown);
    return 0;
}
