using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Xunit;

namespace XxmiLang.Tests;

public class BinaryLocatorTests
{
    private static readonly string Sep = Path.PathSeparator.ToString();

    private static string Resolve(
        HashSet<string> existing,
        string? explicitPath = null,
        string? environment = null,
        string[]? assemblyDirectories = null,
        string? rid = "linux-x64",
        string? path = null,
        bool windows = false) =>
        XxmiBinaryLocator.Resolve(
            explicitPath,
            environment,
            assemblyDirectories ?? new[] { "/app" },
            rid,
            path,
            windows,
            existing.Contains);

    private static string P(params string[] parts) => Path.Combine(parts);

    [Fact]
    public void ExecutablePath_beats_everything()
    {
        var existing = new HashSet<string> { "/x/xxmi", "/env/xxmi", P("/app", "runtimes", "linux-x64", "native", "xxmi"), P("/bin", "xxmi") };

        var found = Resolve(existing, explicitPath: "/x/xxmi", environment: "/env/xxmi", path: "/bin");

        Assert.Equal(Path.GetFullPath("/x/xxmi"), found);
    }

    [Fact]
    public void The_environment_variable_comes_second_and_may_name_a_folder()
    {
        var existing = new HashSet<string> { P("/env", "xxmi"), P("/app", "runtimes", "linux-x64", "native", "xxmi") };

        Assert.Equal(Path.GetFullPath("/env/xxmi"), Resolve(existing, environment: "/env"));
        Assert.Equal(Path.GetFullPath("/env/xxmi"), Resolve(existing, environment: P("/env", "xxmi")));
    }

    [Fact]
    public void On_Windows_an_explicit_path_without_exe_finds_the_exe()
    {
        var existing = new HashSet<string> { P("/tools", "xxmi.exe") };

        Assert.Equal(Path.GetFullPath(P("/tools", "xxmi.exe")), Resolve(existing, explicitPath: P("/tools", "xxmi"), rid: "win-x64", windows: true));
        Assert.Equal(Path.GetFullPath(P("/tools", "xxmi.exe")), Resolve(existing, environment: P("/tools", "xxmi"), rid: "win-x64", windows: true));
        Assert.Throws<XxmiBinaryNotFoundException>(() => Resolve(existing, explicitPath: P("/tools", "xxmi")));
    }

    [Fact]
    public void The_package_binary_comes_third()
    {
        var packaged = P("/app", "runtimes", "linux-x64", "native", "xxmi");
        var existing = new HashSet<string> { packaged, P("/bin", "xxmi") };

        Assert.Equal(packaged, Resolve(existing, path: "/bin"));
    }

    [Fact]
    public void A_binary_published_next_to_the_assembly_is_found()
    {
        var existing = new HashSet<string> { P("/app", "xxmi") };

        Assert.Equal(P("/app", "xxmi"), Resolve(existing));
    }

    [Fact]
    public void PATH_is_the_last_resort_and_uses_the_exe_suffix_on_Windows()
    {
        var existing = new HashSet<string> { P("/second", "xxmi.exe") };

        var found = Resolve(existing, rid: "win-x64", path: "/first" + Sep + "/second", windows: true);

        Assert.Equal(P("/second", "xxmi.exe"), found);
    }

    [Fact]
    public void The_package_lookup_uses_the_runtime_id()
    {
        var existing = new HashSet<string> { P("/app", "runtimes", "osx-arm64", "native", "xxmi") };

        Assert.Throws<XxmiBinaryNotFoundException>(() => Resolve(existing, rid: "linux-x64"));
        Assert.NotNull(Resolve(existing, rid: "osx-arm64"));
    }

    [Fact]
    public void An_explicit_path_that_does_not_exist_is_an_error_not_a_fallthrough()
    {
        var existing = new HashSet<string> { P("/bin", "xxmi") };

        var error = Assert.Throws<XxmiBinaryNotFoundException>(() => Resolve(existing, explicitPath: "/typo/xxmi", path: "/bin"));
        Assert.Contains("/typo/xxmi", error.Message);

        error = Assert.Throws<XxmiBinaryNotFoundException>(() => Resolve(existing, environment: "/typo", path: "/bin"));
        Assert.Contains("XXMI_PATH", error.Message);
    }

    [Fact]
    public void When_nothing_is_found_the_message_lists_every_place_that_was_tried()
    {
        var error = Assert.Throws<XxmiBinaryNotFoundException>(() => Resolve(new HashSet<string>(), path: "/a" + Sep + "/b"));

        Assert.Equal(5, error.Tried.Count); // explicit, env, nested + flat for one directory, PATH
        Assert.Contains("XxmiOptions.ExecutablePath", error.Message);
        Assert.Contains("XXMI_PATH", error.Message);
        Assert.Contains("runtimes", error.Message);
        Assert.Contains("PATH: no xxmi in 2 folders", error.Message);
        Assert.Contains("XxmiLang.Native.", error.Message);
    }

    [Fact]
    public void A_machine_without_a_build_is_explained()
    {
        var error = Assert.Throws<XxmiBinaryNotFoundException>(() => Resolve(new HashSet<string>(), rid: null));

        Assert.Contains("no xxmi build for", error.Message);
    }

    [Fact]
    public void The_current_runtime_id_is_one_of_the_shipped_ones()
    {
        var shipped = new[] { "win-x64", "win-arm64", "linux-x64", "linux-arm64", "osx-x64", "osx-arm64" };

        Assert.Contains(XxmiBinaryLocator.CurrentRuntimeIdentifier(), shipped);
    }

    [Fact]
    public void A_binary_without_the_exec_bit_gets_it()
    {
        if (OperatingSystem.IsWindows()) return; // Unix permissions only
        var directory = Path.Combine(Path.GetTempPath(), "xxmi-exec-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var file = Path.Combine(directory, "xxmi");
            File.WriteAllText(file, "not really xxmi");
            File.SetUnixFileMode(file, UnixFileMode.UserRead | UnixFileMode.UserWrite);

            var found = XxmiBinaryLocator.Resolve(file);

            Assert.Equal(file, found);
            Assert.NotEqual(UnixFileMode.None, File.GetUnixFileMode(file) & UnixFileMode.UserExecute);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
}
