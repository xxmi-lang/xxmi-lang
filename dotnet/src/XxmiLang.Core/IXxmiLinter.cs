using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace XxmiLang
{
    /// <summary>
    /// One-shot linting of files, folders and in-memory text. Implement it to fake the linter in your own
    /// tests; <c>XxmiLinter</c> in the XxmiLang package is the implementation that runs the <c>xxmi</c> binary.
    /// </summary>
    /// <remarks>
    /// Findings are not an error: a result with error-severity diagnostics is returned normally
    /// (<see cref="LintResult.HasErrors"/>). Implementations throw for failures to lint at all.
    /// Paths may be relative; the implementation says what they are relative to.
    /// </remarks>
    public interface IXxmiLinter
    {
        /// <summary>
        /// Batch: lints files and folders (folders recursively, skipping what the package excludes, such as <c>DISABLED*</c>).
        /// An empty list is an <see cref="System.ArgumentException"/>; pass "." to lint the working directory.
        /// </summary>
        Task<LintResult> LintAsync(IEnumerable<string> paths, LintOptions? options = null, CancellationToken cancellationToken = default);

        /// <summary>Lints one file from disk. The result has that one file.</summary>
        Task<LintResult> LintFileAsync(string path, LintOptions? options = null, CancellationToken cancellationToken = default);

        /// <summary>
        /// Lints <paramref name="text"/> as if it were the file at <paramref name="path"/>: the path decides the namespace and
        /// the package (the nearest <c>d3dx.ini</c> above it) and is the one reported; it doesn't have to exist, and the
        /// text wins over what is on disk. For unsaved buffers and generated content.
        /// </summary>
        Task<LintResult> LintTextAsync(string path, string text, LintOptions? options = null, CancellationToken cancellationToken = default);
    }
}
