using System;
using System.Collections.Generic;

namespace XxmiLang
{
    /// <summary>Base class of every exception this library throws on purpose.</summary>
    public class XxmiException : Exception
    {
        public XxmiException(string message) : base(message)
        {
        }

        public XxmiException(string message, Exception? innerException) : base(message, innerException)
        {
        }
    }

    /// <summary>No <c>xxmi</c> executable was found. The message lists every place that was tried.</summary>
    public sealed class XxmiBinaryNotFoundException : XxmiException
    {
        public XxmiBinaryNotFoundException(string message, IReadOnlyList<string> tried) : base(message)
        {
            Tried = tried;
        }

        /// <summary>One line per location that was checked, in order.</summary>
        public IReadOnlyList<string> Tried { get; }
    }

    /// <summary><c>xxmi</c> exited with an error code (2 usage error, 3 internal error) or couldn't be started.</summary>
    public sealed class XxmiProcessException : XxmiException
    {
        public XxmiProcessException(string message, int exitCode, string standardError) : base(message)
        {
            ExitCode = exitCode;
            StandardError = standardError;
        }

        public XxmiProcessException(string message, Exception innerException) : base(message, innerException)
        {
            StandardError = string.Empty;
            ExitCode = -1;
        }

        /// <summary>The exit code, or -1 if the process never started.</summary>
        public int ExitCode { get; }

        /// <summary>Everything the process wrote to stderr.</summary>
        public string StandardError { get; }
    }

    /// <summary>The language server process ended while the service was in use. Call <c>StartAsync</c> again to restart it.</summary>
    public sealed class XxmiServerExitedException : XxmiException
    {
        public XxmiServerExitedException(string message, int? exitCode, string standardError) : base(message)
        {
            ExitCode = exitCode;
            StandardError = standardError;
        }

        public int? ExitCode { get; }

        /// <summary>The last lines the server wrote to stderr.</summary>
        public string StandardError { get; }
    }

    /// <summary>The language server answered a request with an error (for example, a refused rename).</summary>
    public sealed class XxmiRequestException : XxmiException
    {
        public XxmiRequestException(string message, int code) : base(message)
        {
            Code = code;
        }

        /// <summary>The JSON-RPC error code.</summary>
        public int Code { get; }
    }
}
