using System;
using System.Collections.Concurrent;
using System.IO;
using System.Runtime.InteropServices;

namespace XxmiLang
{
    /// <summary>NuGet doesn't keep the Unix exec bit, so a binary taken from a package may need <c>chmod +x</c> before it runs.</summary>
    internal static class ExecutablePermissions
    {
        // One attempt per file and process: the binary is resolved on every call.
        private static readonly ConcurrentDictionary<string, bool> Attempted = new ConcurrentDictionary<string, bool>(StringComparer.Ordinal);

        public static void EnsureExecutable(string path)
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows)) return;
            if (!Attempted.TryAdd(path, true)) return;
            try
            {
                var mode = File.GetUnixFileMode(path);
                const UnixFileMode Execute = UnixFileMode.UserExecute | UnixFileMode.GroupExecute | UnixFileMode.OtherExecute;
                if ((mode & UnixFileMode.UserExecute) != 0) return;
                File.SetUnixFileMode(path, mode | Execute);
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is PlatformNotSupportedException)
            {
                // A failure is fine: the file may not be ours, and then it is probably executable already.
            }
        }
    }
}
