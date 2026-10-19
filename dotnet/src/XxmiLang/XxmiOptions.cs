using System;
using System.Collections.Generic;

namespace XxmiLang
{
    /// <summary>Where the <c>xxmi</c> binary is and how to run it. Shared by the linter and the language service.</summary>
    public class XxmiOptions
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
    }

    /// <summary>Options for <see cref="XxmiLinter.LintAsync(IEnumerable{string}, LintOptions?, System.Threading.CancellationToken)"/>.</summary>
    public sealed class LintOptions : XxmiOptions
    {
        /// <summary>Only run these rules (for example <c>XM201</c>). Null or empty: every rule that's enabled.</summary>
        public IReadOnlyList<string>? Rules { get; set; }
    }
}
