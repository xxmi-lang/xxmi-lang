#if !NET5_0_OR_GREATER
namespace System.Runtime.CompilerServices
{
    /// <summary>Lets <c>init</c> accessors and records compile for netstandard2.0.</summary>
    internal static class IsExternalInit
    {
    }
}
#endif
