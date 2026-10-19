using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    /// <summary>
    /// Lints .ini files and folders by running <c>xxmi lint --format json</c>. Nothing about the language is
    /// implemented here; the result is the CLI's, typed.
    /// </summary>
    public static class XxmiLinter
    {
        // Windows allows 32,767 characters on a command line.
        private const int MaxCommandLine = 24_000;

        private static readonly JsonSerializerOptions Json = new JsonSerializerOptions
        {
            PropertyNameCaseInsensitive = true,
            Converters = { new JsonStringEnumConverter() },
        };

        /// <summary>Lints one file or folder.</summary>
        public static Task<LintResult> LintAsync(string path, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            if (path is null) throw new ArgumentNullException(nameof(path));
            return LintAsync(new[] { path }, options, cancellationToken);
        }

        /// <summary>
        /// Lints files and folders (folders recursively, skipping what the package excludes, such as <c>DISABLED*</c>).
        /// Findings are not an error: the returned <see cref="LintResult"/> carries them, even when xxmi exits with code 1.
        /// </summary>
        /// <exception cref="XxmiBinaryNotFoundException">There is no xxmi executable.</exception>
        /// <exception cref="XxmiProcessException">xxmi failed (exit code 2 or higher), for example because a path doesn't exist.</exception>
        public static async Task<LintResult> LintAsync(
            IEnumerable<string> paths,
            LintOptions? options = null,
            CancellationToken cancellationToken = default)
        {
            if (paths is null) throw new ArgumentNullException(nameof(paths));
            options ??= new LintOptions();
            var workingDirectory = Path.GetFullPath(string.IsNullOrEmpty(options.WorkingDirectory) ? Environment.CurrentDirectory : options.WorkingDirectory!);
            var executable = XxmiBinaryLocator.Resolve(options.ExecutablePath);
            var targets = paths.ToList();
            if (targets.Count == 0) targets.Add(".");

            var flags = new List<string> { "lint", "--format", "json" };
            if (options.Rules is { Count: > 0 }) flags.AddRange(new[] { "--rules", string.Join(",", options.Rules) });

            var files = new List<LintFile>();
            foreach (var batch in Batches(targets, flags.Sum(f => f.Length + 3)))
            {
                var arguments = new List<string>(flags);
                arguments.Add("--"); // a path that starts with "-" is still a path
                arguments.AddRange(batch);
                var output = await ProcessRunner.RunAsync(executable, arguments, workingDirectory, cancellationToken).ConfigureAwait(false);
                if (output.ExitCode > 1)
                {
                    var detail = output.StandardError.Trim();
                    throw new XxmiProcessException(
                        $"xxmi lint failed with exit code {output.ExitCode}: {(detail.Length > 0 ? detail : "(no error output)")}",
                        output.ExitCode,
                        output.StandardError);
                }
                files.AddRange(Parse(output, workingDirectory));
            }
            // The same file can come back from several batches.
            return Summarize(files.GroupBy(f => f.Path, StringComparer.Ordinal).Select(g => g.First()).ToList());
        }

        internal static IReadOnlyList<LintFile> Parse(ProcessOutput output, string workingDirectory)
        {
            LintJson? json;
            try
            {
                json = JsonSerializer.Deserialize<LintJson>(output.StandardOutput, Json);
            }
            catch (JsonException ex)
            {
                throw new XxmiProcessException($"xxmi lint printed output that isn't JSON: {ex.Message}", output.ExitCode, output.StandardError);
            }
            if (json is null || json.Files is null)
            {
                throw new XxmiProcessException("xxmi lint printed no result.", output.ExitCode, output.StandardError);
            }
            if (json.Version != 1)
            {
                throw new XxmiProcessException($"xxmi lint JSON version {json.Version} is not supported by this version of XxmiLang (expected 1).", output.ExitCode, output.StandardError);
            }
            return json.Files
                .Select(f => new LintFile(
                    Path.GetFullPath(Path.Combine(workingDirectory, f.Path)),
                    f.Encoding,
                    f.Diagnostics ?? (IReadOnlyList<XxmiDiagnostic>)Array.Empty<XxmiDiagnostic>()))
                .ToList();
        }

        private static LintResult Summarize(List<LintFile> files)
        {
            int Count(DiagnosticSeverity severity) => files.Sum(f => f.Diagnostics.Count(d => d.Severity == severity));
            return new LintResult(
                files,
                new LintSummary(files.Count, Count(DiagnosticSeverity.Error), Count(DiagnosticSeverity.Warning), Count(DiagnosticSeverity.Info), Count(DiagnosticSeverity.Hint)));
        }

        private static IEnumerable<List<string>> Batches(List<string> paths, int fixedLength)
        {
            var batch = new List<string>();
            var length = fixedLength;
            foreach (var path in paths)
            {
                if (batch.Count > 0 && length + path.Length + 3 > MaxCommandLine)
                {
                    yield return batch;
                    batch = new List<string>();
                    length = fixedLength;
                }
                batch.Add(path);
                length += path.Length + 3;
            }
            if (batch.Count > 0) yield return batch;
        }

        // The wire shape of packages/cli/schema/lint-output.v1.json.
        private sealed record LintJson(int Version, List<LintFileJson> Files);

        private sealed record LintFileJson(string Path, string Encoding, List<XxmiDiagnostic>? Diagnostics);
    }
}
