# XxmiLang.Native

The `xxmi` binary (self-contained, single file, no Node or Bun needed) for one platform, used by the
[`XxmiLang`](https://www.nuget.org/packages/XxmiLang) package: a linter and language service client for
**XXMI / 3DMigoto mod `.ini` files**. This package holds `runtimes/<rid>/native/xxmi[.exe]` and no
assembly.

```bash
dotnet add package XxmiLang
dotnet add package XxmiLang.Native.win-x64      # or win-arm64, linux-x64, linux-arm64, osx-x64, osx-arm64
```

Add one `XxmiLang.Native.<rid>` per platform your app runs on. Full guide:
[docs/10-dotnet.md](https://github.com/xxmi-lang/xxmi-lang/blob/main/docs/10-dotnet.md).
