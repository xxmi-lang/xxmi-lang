using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    /// <summary>
    /// Lints .ini files, folders and in-memory text by running <c>xxmi lint --format json</c> once per call. Nothing about
    /// the language is implemented here; the result is the CLI's, typed. Stateless and thread-safe.
    /// </summary>
    public sealed class XxmiLinter : IXxmiLinter
    {
        // Windows allows 32,767 characters on a command line.
        private const int MaxCommandLine = 24_000;

        private readonly XxmiOptions _options;

        /// <param name="options">Where the binary is and the working directory; null: find it as <see cref="XxmiBinaryLocator"/> does.</param>
        public XxmiLinter(XxmiOptions? options = null)
        {
            _options = options ?? new XxmiOptions();
        }

        /// <summary>
        /// Lints files and folders (folders recursively, skipping what the package excludes, such as <c>DISABLED*</c>).
        /// Relative paths are resolved against <see cref="XxmiOptions.WorkingDirectory"/>. Findings are not an error:
        /// the returned <see cref="LintResult"/> carries them, even when xxmi exits with code 1.
        /// </summary>
        /// <exception cref="XxmiBinaryNotFoundException">There is no xxmi executable.</exception>
        /// <exception cref="XxmiProcessException">xxmi failed (exit code 2 or higher), for example because a path doesn't exist.</exception>
        public async Task<LintResult> LintAsync(IEnumerable<string> paths, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            if (paths is null) throw new ArgumentNullException(nameof(paths));
            var workingDirectory = WorkingDirectory();
            var executable = XxmiBinaryLocator.Resolve(_options.ExecutablePath);
            var targets = paths.ToList();
            if (targets.Count == 0) targets.Add(".");

            var flags = Flags(options);
            var files = new List<LintFile>();
            foreach (var batch in Batches(targets, flags.Sum(f => f.Length + 3)))
            {
                var arguments = new List<string>(flags);
                arguments.Add("--"); // a path that starts with "-" is still a path
                arguments.AddRange(batch);
                var output = await ProcessRunner.RunAsync(executable, arguments, workingDirectory, cancellationToken).ConfigureAwait(false);
                files.AddRange(Parse(output, workingDirectory));
            }
            // The same file can come back from several batches.
            return Summarize(files.GroupBy(f => f.Path, StringComparer.Ordinal).Select(g => g.First()).ToList());
        }

        /// <summary>Lints one file. A folder is refused (use <see cref="LintAsync"/>).</summary>
        /// <exception cref="ArgumentException"><paramref name="path"/> is a folder.</exception>
        public async Task<LintResult> LintFileAsync(string path, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            if (path is null) throw new ArgumentNullException(nameof(path));
            var full = Path.GetFullPath(Path.Combine(WorkingDirectory(), path));
            if (Directory.Exists(full)) throw new ArgumentException($"{full} is a folder; LintFileAsync lints one file. Use LintAsync for folders.", nameof(path));
            return await LintAsync(new[] { full }, options, cancellationToken).ConfigureAwait(false);
        }

        /// <summary>
        /// Lints <paramref name="text"/> as if it were the file at <paramref name="path"/> (<c>xxmi lint --stdin --stdin-path</c>):
        /// the namespace and the package come from the path, which doesn't have to exist, and the text wins over the disk.
        /// </summary>
        public async Task<LintResult> LintTextAsync(string path, string text, LintOptions? options = null, CancellationToken cancellationToken = default)
        {
            if (path is null) throw new ArgumentNullException(nameof(path));
            if (text is null) throw new ArgumentNullException(nameof(text));
            var workingDirectory = WorkingDirectory();
            var executable = XxmiBinaryLocator.Resolve(_options.ExecutablePath);
            var full = Path.GetFullPath(Path.Combine(workingDirectory, path));

            var arguments = Flags(options);
            arguments.Add("--stdin");
            arguments.Add("--stdin-path");
            arguments.Add(full);
            var output = await ProcessRunner.RunAsync(executable, arguments, workingDirectory, cancellationToken, text).ConfigureAwait(false);
            return Summarize(Parse(output, workingDirectory).ToList());
        }

        private string WorkingDirectory() =>
            Path.GetFullPath(string.IsNullOrEmpty(_options.WorkingDirectory) ? Environment.CurrentDirectory : _options.WorkingDirectory!);

        private static List<string> Flags(LintOptions? options)
        {
            var flags = new List<string> { "lint", "--format", "json" };
            if (options?.Rules is { Count: > 0 } rules) flags.AddRange(new[] { "--rules", string.Join(",", rules) });
            return flags;
        }

        internal static IReadOnlyList<LintFile> Parse(ProcessOutput output, string workingDirectory)
        {
            if (output.ExitCode > 1)
            {
                var detail = output.StandardError.Trim();
                throw new XxmiProcessException(
                    $"xxmi lint failed with exit code {output.ExitCode}: {(detail.Length > 0 ? detail : "(no error output)")}",
                    output.ExitCode,
                    output.StandardError);
            }
            LintOutputJson? json;
            try
            {
                json = JsonSerializer.Deserialize(output.StandardOutput, WireJsonContext.Default.LintOutputJson);
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
    }
}
