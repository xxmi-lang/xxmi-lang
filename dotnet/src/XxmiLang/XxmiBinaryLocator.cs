using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;

namespace XxmiLang
{
    /// <summary>
    /// Finds the <c>xxmi</c> executable. Order:
    /// <list type="number">
    /// <item><description><see cref="XxmiOptions.ExecutablePath"/>.</description></item>
    /// <item><description>The <c>XXMI_PATH</c> environment variable (a file, or a folder containing <c>xxmi</c>).</description></item>
    /// <item><description><c>runtimes/&lt;rid&gt;/native/xxmi[.exe]</c> in the app's folder (<see cref="AppContext.BaseDirectory"/>; what the <c>XxmiLang.Native.*</c> packages install). A
    /// RID-specific publish puts the binary straight into the output folder; that is checked too.</description></item>
    /// <item><description><c>xxmi</c> on <c>PATH</c>.</description></item>
    /// </list>
    /// An explicit setting (1, 2) that points nowhere is an error, not a reason to fall through.
    /// </summary>
    public static class XxmiBinaryLocator
    {
        /// <summary>Name of the environment variable that overrides the lookup.</summary>
        public const string EnvironmentVariable = "XXMI_PATH";

        /// <summary>Returns the full path of the executable or throws <see cref="XxmiBinaryNotFoundException"/>.</summary>
        public static string Resolve(string? explicitPath = null)
        {
            var path = Resolve(
                explicitPath,
                Environment.GetEnvironmentVariable(EnvironmentVariable),
                AssemblyDirectories(),
                CurrentRuntimeIdentifier(),
                Environment.GetEnvironmentVariable("PATH"),
                IsWindows(),
                File.Exists);
            ExecutablePermissions.EnsureExecutable(path);
            return path;
        }

        /// <summary>The .NET runtime identifier of this machine (<c>win-x64</c>, <c>linux-arm64</c>, <c>osx-arm64</c>, …), or null if there is no xxmi build for it.</summary>
        public static string? CurrentRuntimeIdentifier()
        {
            string os;
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows)) os = "win";
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux)) os = "linux";
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX)) os = "osx";
            else return null;
            switch (RuntimeInformation.OSArchitecture)
            {
                case Architecture.X64: return os + "-x64";
                case Architecture.Arm64: return os + "-arm64";
                default: return null;
            }
        }

        internal static string Resolve(
            string? explicitPath,
            string? environmentValue,
            IEnumerable<string> assemblyDirectories,
            string? rid,
            string? pathVariable,
            bool windows,
            Func<string, bool> fileExists)
        {
            var exeName = windows ? "xxmi.exe" : "xxmi";
            var tried = new List<string>();

            string Explicit(string label, string value)
            {
                foreach (var candidate in new[] { value, Path.Combine(value, exeName) })
                {
                    if (fileExists(candidate)) return Path.GetFullPath(candidate);
                }
                tried.Add($"{label}: {value} (not found)");
                throw NotFound(tried, explicitFailure: true);
            }

            if (!string.IsNullOrWhiteSpace(explicitPath)) return Explicit("XxmiOptions.ExecutablePath", explicitPath!);
            tried.Add("XxmiOptions.ExecutablePath: not set");

            if (!string.IsNullOrWhiteSpace(environmentValue)) return Explicit(EnvironmentVariable, environmentValue!);
            tried.Add($"{EnvironmentVariable}: not set");

            var directories = assemblyDirectories.Where(d => !string.IsNullOrEmpty(d)).Distinct().ToList();
            if (rid is null)
            {
                tried.Add($"runtimes/<rid>/native/{exeName}: no xxmi build for {RuntimeInformation.OSDescription} {RuntimeInformation.OSArchitecture}");
            }
            else
            {
                foreach (var directory in directories)
                {
                    var nested = Path.Combine(directory, "runtimes", rid, "native", exeName);
                    if (fileExists(nested)) return nested;
                    tried.Add($"package binary: {nested} (not found)");
                    var flat = Path.Combine(directory, exeName);
                    if (fileExists(flat)) return flat;
                    tried.Add($"published binary: {flat} (not found)");
                }
            }

            var searched = 0;
            foreach (var directory in (pathVariable ?? string.Empty).Split(Path.PathSeparator))
            {
                var trimmed = directory.Trim().Trim('"');
                if (trimmed.Length == 0) continue;
                searched++;
                string candidate;
                try
                {
                    candidate = Path.Combine(trimmed, exeName);
                }
                catch (ArgumentException)
                {
                    continue; // an invalid character in a PATH entry
                }
                if (fileExists(candidate)) return candidate;
            }
            tried.Add($"PATH: no {exeName} in {searched} folder{(searched == 1 ? string.Empty : "s")}");
            throw NotFound(tried, explicitFailure: false);
        }

        private static XxmiBinaryNotFoundException NotFound(List<string> tried, bool explicitFailure)
        {
            var message = explicitFailure
                ? "The xxmi executable you pointed to doesn't exist: " + tried.Last()
                : "Could not find the xxmi executable. Tried, in order:" + Environment.NewLine
                  + string.Join(Environment.NewLine, tried.Select(t => "  - " + t)) + Environment.NewLine
                  + "Install a runtime package (for example XxmiLang.Native." + (CurrentRuntimeIdentifier() ?? "<rid>")
                  + " or XxmiLang.All), set XxmiOptions.ExecutablePath, or set " + EnvironmentVariable + ".";
            return new XxmiBinaryNotFoundException(message, tried);
        }

        private static bool IsWindows() => RuntimeInformation.IsOSPlatform(OSPlatform.Windows);

        private static IEnumerable<string> AssemblyDirectories()
        {
            // The app's folder: where the runtime package puts runtimes/<rid>/native/ for a build or a framework-dependent
            // publish, and the binary itself for a `-r <rid>` publish (single-file and Native AOT included).
            yield return AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }
    }
}
