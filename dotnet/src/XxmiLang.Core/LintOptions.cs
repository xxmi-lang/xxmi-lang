using System.Collections.Generic;

namespace XxmiLang
{
    /// <summary>What to lint for. Where the <c>xxmi</c> binary is belongs to the implementation (<c>XxmiOptions</c> in the XxmiLang package).</summary>
    public sealed class LintOptions
    {
        /// <summary>Only run these rules (for example <c>XM201</c>). Null or empty: every rule that's enabled.</summary>
        public IReadOnlyList<string>? Rules { get; set; }
    }
}
