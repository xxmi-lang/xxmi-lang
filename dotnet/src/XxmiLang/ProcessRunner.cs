using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    internal sealed record ProcessOutput(int ExitCode, string StandardOutput, string StandardError);

    internal static class ProcessRunner
    {
        public static ProcessStartInfo CreateStartInfo(string executable, IReadOnlyList<string> arguments, string? workingDirectory)
        {
            var start = new ProcessStartInfo
            {
                FileName = executable,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                StandardOutputEncoding = new UTF8Encoding(false),
                StandardErrorEncoding = new UTF8Encoding(false),
                WorkingDirectory = string.IsNullOrEmpty(workingDirectory) ? Environment.CurrentDirectory : workingDirectory!,
            };
#if NET5_0_OR_GREATER
            foreach (var argument in arguments) start.ArgumentList.Add(argument);
#else
            start.Arguments = JoinArguments(arguments);
#endif
            return start;
        }

        /// <summary>Starts the process, mapping the failures people hit (no exec bit, Gatekeeper, wrong architecture) to a readable exception.</summary>
        public static Process Start(ProcessStartInfo start)
        {
            try
            {
                return Process.Start(start) ?? throw new XxmiProcessException($"Could not start {start.FileName}.", new InvalidOperationException("Process.Start returned null."));
            }
            catch (Exception ex) when (ex is Win32Exception || ex is InvalidOperationException || ex is System.IO.IOException)
            {
                throw new XxmiProcessException(
                    $"Could not start {start.FileName}: {ex.Message}. On Linux and macOS the file must be executable (chmod +x); on macOS an unsigned binary that was downloaded in a browser may need its quarantine flag removed (xattr -d com.apple.quarantine). See docs/10-dotnet.md.",
                    ex);
            }
        }

        public static async Task<ProcessOutput> RunAsync(
            string executable,
            IReadOnlyList<string> arguments,
            string? workingDirectory,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            using (var process = Start(CreateStartInfo(executable, arguments, workingDirectory)))
            using (cancellationToken.Register(() => Kill(process)))
            {
                process.StandardInput.Close();
                var output = process.StandardOutput.ReadToEndAsync();
                var error = process.StandardError.ReadToEndAsync();
                var text = await output.ConfigureAwait(false);
                var errorText = await error.ConfigureAwait(false);
                process.WaitForExit();
                cancellationToken.ThrowIfCancellationRequested();
                return new ProcessOutput(process.ExitCode, text, errorText);
            }
        }

        public static void Kill(Process process)
        {
            try
            {
                if (process.HasExited) return;
#if NET5_0_OR_GREATER
                process.Kill(entireProcessTree: true);
#else
                process.Kill();
#endif
            }
            catch (Exception ex) when (ex is InvalidOperationException || ex is Win32Exception || ex is NotSupportedException)
            {
                // Already gone.
            }
        }

        /// <summary>Quotes arguments the way CommandLineToArgvW (and .NET on Unix) reads them back.</summary>
        internal static string JoinArguments(IEnumerable<string> arguments)
        {
            var result = new StringBuilder();
            foreach (var argument in arguments)
            {
                if (result.Length > 0) result.Append(' ');
                if (argument.Length > 0 && argument.IndexOfAny(new[] { ' ', '\t', '\n', '\v', '"' }) < 0)
                {
                    result.Append(argument);
                    continue;
                }
                result.Append('"');
                var backslashes = 0;
                foreach (var c in argument)
                {
                    if (c == '\\')
                    {
                        backslashes++;
                        continue;
                    }
                    if (c == '"') result.Append('\\', backslashes * 2 + 1).Append('"');
                    else result.Append('\\', backslashes).Append(c);
                    backslashes = 0;
                }
                result.Append('\\', backslashes * 2).Append('"');
            }
            return result.ToString();
        }
    }
}
