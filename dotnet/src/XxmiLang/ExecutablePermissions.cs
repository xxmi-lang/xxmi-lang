using System;
using System.Collections.Concurrent;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;

namespace XxmiLang
{
    /// <summary>NuGet doesn't keep the Unix exec bit, so a binary taken from a package may need <c>chmod +x</c> before it runs.</summary>
    internal static class ExecutablePermissions
    {
        // One attempt per file and process: LintAsync resolves the binary on every call.
        private static readonly ConcurrentDictionary<string, bool> Attempted = new ConcurrentDictionary<string, bool>(StringComparer.Ordinal);

        public static void EnsureExecutable(string path)
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows)) return;
            if (!Attempted.TryAdd(path, true)) return;
            try
            {
#if NET7_0_OR_GREATER
                var mode = File.GetUnixFileMode(path);
                const UnixFileMode Execute = UnixFileMode.UserExecute | UnixFileMode.GroupExecute | UnixFileMode.OtherExecute;
                if ((mode & UnixFileMode.UserExecute) != 0) return;
                File.SetUnixFileMode(path, mode | Execute);
#else
                // 0755. A failure is fine: the file may not be ours, and then it is probably executable already.
                if (!TryChmod(path, 493)) RunChmod(path);
#endif
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is PlatformNotSupportedException)
            {
            }
        }

#if !NET7_0_OR_GREATER
        [DllImport("libc", EntryPoint = "chmod", SetLastError = true)]
        private static extern int Chmod([MarshalAs(UnmanagedType.LPStr)] string path, uint mode);

        private static bool TryChmod(string path, uint mode)
        {
            try
            {
                return Chmod(path, mode) == 0;
            }
            catch (Exception ex) when (ex is DllNotFoundException || ex is EntryPointNotFoundException)
            {
                return false;
            }
        }

        private static void RunChmod(string path)
        {
            try
            {
                var start = new ProcessStartInfo("chmod", "u+x \"" + path.Replace("\"", "\\\"") + "\"")
                {
                    UseShellExecute = false,
                    CreateNoWindow = true,
                };
                using (var process = Process.Start(start))
                {
                    process?.WaitForExit(5000);
                }
            }
            catch (Win32Exception)
            {
            }
        }
#endif
    }
}
