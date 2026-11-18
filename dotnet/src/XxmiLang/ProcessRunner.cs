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
            string? standardInput = null,
            TimeSpan? timeout = null)
        {
            cancellationToken.ThrowIfCancellationRequested();
            using (var limit = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
            {
                if (timeout is { } span) limit.CancelAfter(span);
                try
                {
                    return await RunCoreAsync(executable, arguments, workingDirectory, standardInput, limit.Token).ConfigureAwait(false);
                }
                catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
                {
                    throw new XxmiProcessException(
                        $"{Path.GetFileName(executable)} did not finish within {timeout}; it was killed. Raise XxmiOptions.ProcessTimeout if the input is that large.",
                        -1,
                        string.Empty);
                }
            }
        }

        // Every wait takes the token, and cancelling it kills the whole process tree first: a child that never exits,
        // or a grandchild that keeps the pipes open, then can't leave the caller waiting for end of output.
        private static async Task<ProcessOutput> RunCoreAsync(
            string executable,
            IReadOnlyList<string> arguments,
            string? workingDirectory,
            string? standardInput,
            CancellationToken cancellationToken)
        {
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
                try
                {
                    await Task.WhenAll(output, error).WaitAsync(cancellationToken).ConfigureAwait(false);
                    await process.WaitForExitAsync(cancellationToken).ConfigureAwait(false);
                }
                catch (OperationCanceledException)
                {
                    // The registration above can lose the race with the exception it causes: it is disposed before it ran.
                    Kill(process);
                    throw;
                }
                return new ProcessOutput(process.ExitCode, output.Result, error.Result);
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
