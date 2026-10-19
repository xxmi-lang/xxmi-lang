using System;
using System.IO;
using XxmiLang;

namespace XxmiLang.Tests;

/// <summary>Finds the repo, its fixtures and the xxmi binary the tests run against.</summary>
internal static class TestEnvironment
{
    public static readonly string RepoRoot = FindRepoRoot();

    public static string Package => Path.Combine(RepoRoot, "fixtures", "lsp", "package");

    public static string Mod => Path.Combine(Package, "Mods", "MyMod");

    public static string ModIni => Path.Combine(Mod, "mod.ini");

    public static string LibIni => Path.Combine(Package, "Core", "Lib", "lib.ini");

    /// <summary>XXMI_PATH if set (CI sets it), else the binary <c>pnpm build:binaries --target host</c> leaves in dist/bin.</summary>
    public static XxmiOptions Options()
    {
        var env = Environment.GetEnvironmentVariable(XxmiBinaryLocator.EnvironmentVariable);
        if (!string.IsNullOrWhiteSpace(env)) return new XxmiOptions { ExecutablePath = env };
        var local = Path.Combine(RepoRoot, "dist", "bin", OperatingSystem.IsWindows() ? "xxmi.exe" : "xxmi");
        if (!File.Exists(local))
        {
            throw new InvalidOperationException(
                $"No xxmi binary for the tests. Run `pnpm build:binaries --target host` in {RepoRoot}, or set XXMI_PATH.");
        }
        return new XxmiOptions { ExecutablePath = local };
    }

    public static XxmiLinter Linter(string? workingDirectory = null)
    {
        var options = Options();
        options.WorkingDirectory = workingDirectory;
        return new XxmiLinter(options);
    }

    /// <summary>A throwaway folder with a mod.ini holding <paramref name="text"/>.</summary>
    public static string TempMod(string text)
    {
        var directory = Path.Combine(Path.GetTempPath(), "xxmi-lang-tests-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        File.WriteAllText(Path.Combine(directory, "mod.ini"), text);
        return directory;
    }

    private static string FindRepoRoot()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory != null; directory = directory.Parent)
        {
            if (File.Exists(Path.Combine(directory.FullName, "pnpm-workspace.yaml"))) return directory.FullName;
        }
        throw new InvalidOperationException("Run the tests from inside the xxmi-lang repository.");
    }
}
