using System;
using Xunit;

namespace XxmiLang.Core.Tests;

public class TextEditTests
{
    private static TextEdit Edit(int l1, int c1, int l2, int c2, string text) =>
        new TextEdit(new TextRange(new TextPosition(l1, c1), new TextPosition(l2, c2)), text);

    [Fact]
    public void Edits_refer_to_the_original_text_in_any_order()
    {
        var result = TextEdits.Apply("one\ntwo\nthree", new[] { Edit(2, 0, 2, 5, "3"), Edit(0, 0, 0, 3, "1") });

        Assert.Equal("1\ntwo\n3", result);
    }

    [Fact]
    public void Inserts_and_CRLF_work()
    {
        Assert.Equal("a\r\nXb\r\n", TextEdits.Apply("a\r\nb\r\n", new[] { Edit(1, 0, 1, 0, "X") }));
    }

    [Fact]
    public void Overlapping_edits_are_refused()
    {
        Assert.Throws<ArgumentException>(() => TextEdits.Apply("abcdef", new[] { Edit(0, 0, 0, 4, "x"), Edit(0, 2, 0, 5, "y") }));
    }
}
