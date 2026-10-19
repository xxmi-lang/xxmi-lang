using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

namespace XxmiLang
{
    /// <summary>Applies the edits the library returns (quick fixes, renames).</summary>
    public static class TextEdits
    {
        /// <summary>
        /// Applies <paramref name="edits"/> to <paramref name="text"/>. All ranges refer to the original text,
        /// as in LSP, so the order of the edits doesn't matter. Overlapping edits throw.
        /// </summary>
        public static string Apply(string text, IEnumerable<TextEdit> edits)
        {
            if (text is null) throw new ArgumentNullException(nameof(text));
            if (edits is null) throw new ArgumentNullException(nameof(edits));
            var lineStarts = LineStarts(text);
            int Offset(TextPosition p)
            {
                if (p.Line >= lineStarts.Count) return text.Length;
                var offset = lineStarts[p.Line] + p.Character;
                var lineEnd = p.Line + 1 < lineStarts.Count ? lineStarts[p.Line + 1] : text.Length;
                return Math.Min(offset, lineEnd);
            }

            var ordered = edits
                .Select(e => (Start: Offset(e.Range.Start), End: Offset(e.Range.End), e.NewText))
                .OrderBy(e => e.Start)
                .ThenBy(e => e.End)
                .ToList();
            var result = new StringBuilder();
            var position = 0;
            foreach (var (start, end, newText) in ordered)
            {
                if (start < position) throw new ArgumentException("The edits overlap.", nameof(edits));
                result.Append(text, position, start - position).Append(newText);
                position = Math.Max(end, start);
            }
            return result.Append(text, position, text.Length - position).ToString();
        }

        private static List<int> LineStarts(string text)
        {
            // LSP line breaks: \n, \r\n and a lone \r.
            var starts = new List<int> { 0 };
            for (var i = 0; i < text.Length; i++)
            {
                if (text[i] == '\r' && i + 1 < text.Length && text[i + 1] == '\n') i++;
                else if (text[i] != '\r' && text[i] != '\n') continue;
                starts.Add(i + 1);
            }
            return starts;
        }
    }
}
