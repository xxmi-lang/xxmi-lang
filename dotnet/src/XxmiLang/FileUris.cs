using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace XxmiLang
{
    /// <summary>Converts between local paths and the <c>file:</c> URIs LSP uses (same shape as Node's pathToFileURL).</summary>
    internal static class FileUris
    {
        public static string FromPath(string path, bool windows)
        {
            var full = windows ? path.Replace('\\', '/') : path;
            var result = new StringBuilder("file://");
            if (windows && full.StartsWith("//", StringComparison.Ordinal))
            {
                full = full.Substring(2); // \\host\share\x -> file://host/share/x
            }
            else if (!full.StartsWith("/", StringComparison.Ordinal))
            {
                result.Append('/'); // C:/x -> file:///C:/x
            }
            foreach (var b in Encoding.UTF8.GetBytes(full))
            {
                var c = (char)b;
                var safe = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || "-._~/:@!$&'()*+,;=".IndexOf(c) >= 0;
                if (safe && b < 0x80) result.Append(c);
                else result.Append('%').Append(b.ToString("X2"));
            }
            return result.ToString();
        }

        public static string FromPath(string path) => FromPath(Path.GetFullPath(path), RuntimeInformation.IsOSPlatform(OSPlatform.Windows));

        public static string ToPath(string uri, bool windows)
        {
            if (!uri.StartsWith("file:", StringComparison.OrdinalIgnoreCase)) throw new ArgumentException("Not a file: URI: " + uri, nameof(uri));
            var rest = uri.Substring("file:".Length);
            string host = string.Empty;
            if (rest.StartsWith("//", StringComparison.Ordinal))
            {
                rest = rest.Substring(2);
                var slash = rest.IndexOf('/');
                host = slash < 0 ? rest : rest.Substring(0, slash);
                rest = slash < 0 ? string.Empty : rest.Substring(slash);
            }
            var path = Uri.UnescapeDataString(rest);
            if (!windows) return path;
            if (host.Length > 0 && !host.Equals("localhost", StringComparison.OrdinalIgnoreCase))
            {
                return "\\\\" + host + path.Replace('/', '\\');
            }
            if (path.Length >= 3 && path[0] == '/' && path[2] == ':') path = path.Substring(1); // /C:/x -> C:/x
            return path.Replace('/', '\\');
        }

        public static string ToPath(string uri) => ToPath(uri, RuntimeInformation.IsOSPlatform(OSPlatform.Windows));
    }
}
