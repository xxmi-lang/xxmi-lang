using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
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
            foreach (var argument in arguments) start.ArgumentList.Add(argument);
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
            CancellationToken cancellationToken,
            string? standardInput = null)
        {
            cancellationToken.ThrowIfCancellationRequested();
            using (var process = Start(CreateStartInfo(executable, arguments, workingDirectory)))
            using (cancellationToken.Register(() => Kill(process)))
            {
                // Reading starts before the input is written, so a child that answers while it is still reading can't block.
                var output = process.StandardOutput.ReadToEndAsync();
                var error = process.StandardError.ReadToEndAsync();
                try
                {
                    if (standardInput is { Length: > 0 })
                    {
                        var bytes = new UTF8Encoding(false).GetBytes(standardInput);
                        await process.StandardInput.BaseStream.WriteAsync(bytes, cancellationToken).ConfigureAwait(false);
                        await process.StandardInput.BaseStream.FlushAsync(cancellationToken).ConfigureAwait(false);
                    }
                }
                catch (IOException)
                {
                    // The process ended before it read everything; its exit code and stderr say why.
                }
                finally
                {
                    try
                    {
                        process.StandardInput.Close();
                    }
                    catch (IOException)
                    {
                    }
                }
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
                process.Kill(entireProcessTree: true);
            }
            catch (Exception ex) when (ex is InvalidOperationException || ex is Win32Exception || ex is NotSupportedException)
            {
                // Already gone.
            }
        }
    }
}
