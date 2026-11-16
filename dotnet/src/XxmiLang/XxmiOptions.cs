using System;

namespace XxmiLang
{
    /// <summary>Where the <c>xxmi</c> binary is and how to run it. Shared by <see cref="XxmiLinter"/> and <see cref="XxmiLanguageService"/>.</summary>
    public sealed class XxmiOptions
    {
        /// <summary>
        /// Full path of the <c>xxmi</c> executable (or the folder holding it). Wins over everything else;
        /// if it is set but doesn't exist, resolution fails instead of silently using another binary.
        /// See <see cref="XxmiBinaryLocator"/> for the full order.
        /// </summary>
        public string? ExecutablePath { get; set; }

        /// <summary>
        /// Working directory of the process. Defaults to the current directory. Relative paths you pass
        /// to the linter are resolved against it.
        /// </summary>
        public string? WorkingDirectory { get; set; }

        /// <summary>How long a language service request may take before it fails with <see cref="TimeoutException"/>. Default: 60 seconds.</summary>
        public TimeSpan RequestTimeout { get; set; } = TimeSpan.FromSeconds(60);

        /// <summary>How long the language service waits for the server to shut down (the handshake and the exit together) before it kills the process tree. Default: 10 seconds.</summary>
        public TimeSpan ShutdownTimeout { get; set; } = TimeSpan.FromSeconds(10);

        /// <summary>
        /// How long one <see cref="XxmiLinter"/> process may run before it is killed and <see cref="XxmiProcessException"/> is thrown;
        /// each batch of a <c>LintAsync</c> call gets this much. Default: 2 minutes.
        /// </summary>
        public TimeSpan ProcessTimeout { get; set; } = TimeSpan.FromMinutes(2);
    }
}
