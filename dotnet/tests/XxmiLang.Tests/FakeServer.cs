using System;
using System.IO;

namespace XxmiLang.Tests;

/// <summary>
/// The `XxmiLang.FakeServer` executable in one of its misbehaving modes (listed in its Program.cs), wired into options
/// with short timeouts. The mode travels in the working directory because the environment is shared by parallel tests.
/// </summary>
internal sealed class FakeServer : IDisposable
{
    private FakeServer(string mode, TimeSpan requestTimeout, TimeSpan processTimeout)
    {
        Directory = Path.Combine(Path.GetTempPath(), "xxmi-lang-fake-" + Guid.NewGuid().ToString("N"));
        System.IO.Directory.CreateDirectory(Directory);
        File.WriteAllText(Path.Combine(Directory, "mode"), mode);
        Options = new XxmiOptions
        {
            ExecutablePath = ExecutablePath(),
            WorkingDirectory = Directory,
            RequestTimeout = requestTimeout,
            ShutdownTimeout = TimeSpan.FromSeconds(1),
            ProcessTimeout = processTimeout,
        };
    }

    public string Directory { get; }

    public XxmiOptions Options { get; }

    // A healthy start must fit in the default on a slow runner; only a test about that very timeout passes a short one.
    public static FakeServer InMode(string mode, TimeSpan? requestTimeout = null, TimeSpan? processTimeout = null) =>
        new FakeServer(mode, requestTimeout ?? DefaultTimeout, processTimeout ?? DefaultTimeout);

    private static readonly TimeSpan DefaultTimeout = TimeSpan.FromSeconds(10);

    public void Dispose()
    {
        try
        {
            System.IO.Directory.Delete(Directory, recursive: true);
        }
        catch (IOException)
        {
            // A killed process may still hold its working directory for a moment; the temp folder is not worth a failure.
        }
    }

    // The project builds next to the test project: artifacts/bin/<project>/<configuration>/<framework>/.
    private static string ExecutablePath()
    {
        var framework = new DirectoryInfo(AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
        var configuration = framework.Parent!;
        var bin = configuration.Parent!.Parent!;
        var name = "XxmiLang.FakeServer" + (OperatingSystem.IsWindows() ? ".exe" : string.Empty);
        return Path.Combine(bin.FullName, "XxmiLang.FakeServer", configuration.Name, framework.Name, name);
    }
}
