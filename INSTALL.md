# Installing xxmi-lsp and xxmi

Two programs, both single files with nothing else to install (no Node, no Python):

- **`xxmi-lsp`**, the language server. Editors (Kate, Claude Code, Neovim, …) run it to show
  errors, go to definition, rename and so on.
- **`xxmi`**, the command-line linter (`xxmi lint path/to/mod`). Optional, but handy.

**VS Code users don't need either.** The extension has the server built in; skip to
[VS Code](#vs-code).

Jump to: [Arch / CachyOS](#arch-linux-cachyos-manjaro-endeavouros) ·
[Other Linux](#other-linux-distributions) · [Windows](#windows) · [macOS](#macos) ·
[Editors](#set-up-your-editor) · [Updating](#updating-and-uninstalling) ·
[Troubleshooting](#troubleshooting)

## Which file do I need?

Every release has the files at
[github.com/xxmi-lang/xxmi-lang/releases/latest](https://github.com/xxmi-lang/xxmi-lang/releases/latest):

| System                                      | `xxmi-lsp` file              | `xxmi` file              |
| ------------------------------------------- | ---------------------------- | ------------------------ |
| Linux, Intel/AMD (`uname -m` says `x86_64`) | `xxmi-lsp-linux-x64`         | `xxmi-linux-x64`         |
| Linux, ARM (`aarch64`)                      | `xxmi-lsp-linux-arm64`       | `xxmi-linux-arm64`       |
| Windows, Intel/AMD                          | `xxmi-lsp-windows-x64.exe`   | `xxmi-windows-x64.exe`   |
| Windows on ARM (Snapdragon)                 | `xxmi-lsp-windows-arm64.exe` | `xxmi-windows-arm64.exe` |
| macOS, Apple Silicon (M1 or newer)          | `xxmi-lsp-darwin-arm64`      | `xxmi-darwin-arm64`      |
| macOS, Intel                                | `xxmi-lsp-darwin-x64`        | `xxmi-darwin-x64`        |

The commands below download the right file and rename it to plain `xxmi-lsp` / `xxmi`, which
is the name editors look for.

## Arch Linux, CachyOS, Manjaro, EndeavourOS

There's no AUR package yet; install into `~/.local/bin`, which needs no root.

1. **Download** (Intel/AMD; on ARM replace `linux-x64` with `linux-arm64`):

   ```bash
   mkdir -p ~/.local/bin
   base=https://github.com/xxmi-lang/xxmi-lang/releases/latest/download
   curl -fL -o ~/.local/bin/xxmi-lsp "$base/xxmi-lsp-linux-x64"
   curl -fL -o ~/.local/bin/xxmi     "$base/xxmi-linux-x64"
   chmod +x ~/.local/bin/xxmi-lsp ~/.local/bin/xxmi
   ```

2. **Put `~/.local/bin` on your PATH**, if it isn't already (`echo $PATH` shows it).
   - **CachyOS uses fish by default.** Run once:

     ```fish
     fish_add_path ~/.local/bin
     ```

   - bash: `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc`
   - zsh: `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc`

   Then open a new terminal.

3. **Check:**

   ```bash
   xxmi-lsp --version
   xxmi lint --help
   ```

4. **Editor:** for Kate, run `sudo pacman -S kate`, then follow [Kate](#kate). For VS Code,
   see [VS Code](#vs-code); Arch's `code` package and VSCodium both use Open VSX.

## Other Linux distributions

Debian, Ubuntu, Mint, Pop!_OS, Fedora, openSUSE, Bazzite and others use the same files and the
same steps as [Arch](#arch-linux-cachyos-manjaro-endeavouros). Only the editor install differs:

| Distro                 | Kate                                   | curl, if missing        |
| ---------------------- | -------------------------------------- | ----------------------- |
| Debian / Ubuntu / Mint | `sudo apt install kate`                | `sudo apt install curl` |
| Fedora                 | `sudo dnf install kate`                | (installed)             |
| openSUSE               | `sudo zypper install kate`             | (installed)             |
| Any (Flatpak)          | `flatpak install flathub org.kde.kate` |                         |

On Ubuntu and Fedora, `~/.local/bin` is already on the PATH once the folder exists; log out and
back in after creating it.

**Flatpak Kate** runs in a sandbox and can't see `~/.local/bin` directly. In its LSP settings,
try starting the host binary through `flatpak-spawn` (untested; it needs the Kate Flatpak to be
allowed to talk to the host):
`"command": ["flatpak-spawn", "--host", "xxmi-lsp", "--stdio"]`. If that doesn't work, the
distro's Kate package is the simpler route.

**Not supported:** Alpine and other musl-based systems (the binaries need glibc).

## Windows

In **PowerShell** (not as admin). On Windows on ARM, replace `windows-x64` with
`windows-arm64`.

```powershell
$dir = "$env:LOCALAPPDATA\Programs\xxmi"
New-Item -ItemType Directory -Force $dir | Out-Null
$base = 'https://github.com/xxmi-lang/xxmi-lang/releases/latest/download'
Invoke-WebRequest "$base/xxmi-lsp-windows-x64.exe" -OutFile "$dir\xxmi-lsp.exe"
Invoke-WebRequest "$base/xxmi-windows-x64.exe"     -OutFile "$dir\xxmi.exe"
Unblock-File "$dir\xxmi-lsp.exe", "$dir\xxmi.exe"

# Add the folder to your user PATH (once).
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
}
```

Close and reopen the terminal (and your editor), then check with `xxmi-lsp --version`.

If you downloaded the files with a browser instead, Windows SmartScreen may warn the first time
you run them, because they aren't code-signed. Choose "More info" → "Run anyway", or run
`Unblock-File` on them as above.

## macOS

Apple Silicon (M1 or newer). On an Intel Mac, replace `darwin-arm64` with `darwin-x64`.

```bash
mkdir -p ~/.local/bin
base=https://github.com/xxmi-lang/xxmi-lang/releases/latest/download
curl -fL -o ~/.local/bin/xxmi-lsp "$base/xxmi-lsp-darwin-arm64"
curl -fL -o ~/.local/bin/xxmi     "$base/xxmi-darwin-arm64"
chmod +x ~/.local/bin/xxmi-lsp ~/.local/bin/xxmi
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc   # once; zsh is the macOS default
```

Open a new terminal and check with `xxmi-lsp --version`.

The binaries aren't notarized by Apple. Files downloaded with `curl` as above run as they are.
If you downloaded them with a browser and macOS says it "can't be opened", either allow it in
System Settings → Privacy & Security → "Open Anyway", or run
`xattr -d com.apple.quarantine ~/.local/bin/xxmi-lsp ~/.local/bin/xxmi`.

## Verify the download (optional)

Each release has a `SHA256SUMS` file. On Linux:

```bash
cd ~/.local/bin
curl -fsSL -o /tmp/SHA256SUMS "$base/SHA256SUMS"
echo "$(grep ' xxmi-lsp-linux-x64$' /tmp/SHA256SUMS | cut -d' ' -f1)  xxmi-lsp" | sha256sum -c
```

On macOS use `shasum -a 256 -c` instead of `sha256sum -c`. On Windows:
`(Get-FileHash "$dir\xxmi-lsp.exe").Hash`, then compare it with the line in `SHA256SUMS`.

## Set up your editor

Open your **ZZMI folder** (the one with `d3dx.ini`), not just a single mod. References like
`run = CommandList\ZZMI\SetTextures` then resolve against your installed libraries. Opening a
single mod works too; library names then come from a copy bundled with the release.

### VS Code

1. Download `xxmi-lang.vsix` from the
   [latest release](https://github.com/xxmi-lang/xxmi-lang/releases/latest) and install it:
   Extensions view → `…` → "Install from VSIX…", or run
   `code --install-extension xxmi-lang.vsix`.
2. For syntax colors, add lupomikti's "3DMigoto INI" extension:
   - **VSCodium, Code-OSS, or Arch's `code` package** (these use Open VSX): run
     `code --install-extension AGMG.migoto-ini` (`codium …` for VSCodium), or search
     "3DMigoto INI" in the Extensions view.
   - **Microsoft VS Code** (it isn't on Microsoft's Marketplace): download the `.vsix` from
     [lupomikti/migoto-vscode releases](https://github.com/lupomikti/migoto-vscode/releases/latest)
     and install it the same way.

The extension doesn't use the `xxmi-lsp` you installed; it has its own copy. To use a specific
binary instead, set `xxmi.lsp.path` in the settings.

### Kate

See [clients/kate/README.md](clients/kate/README.md) for the full steps. In short:

1. Install lupomikti's syntax file for colors:

   ```bash
   mkdir -p ~/.local/share/org.kde.syntax-highlighting/syntax
   curl -fL -o ~/.local/share/org.kde.syntax-highlighting/syntax/migoto.xml \
     https://raw.githubusercontent.com/lupomikti/3dmigoto-ini-extension/main/kate-plugin/syntaxes/migoto.xml
   ```

   On Windows the folder is `%USERPROFILE%\AppData\Local\org.kde.syntax-highlighting\syntax`.

2. Enable Settings → Configure Kate → Plugins → **LSP Client**.
3. In Settings → Configure Kate → LSP Client → **User Server Settings**, paste:

   ```json
   {
     "servers": {
       "migoto": {
         "command": ["xxmi-lsp", "--stdio"],
         "highlightingModeRegex": "^Migoto$",
         "rootIndicationFileNames": ["d3dx.ini", "xxmi.toml"]
       }
     }
   }
   ```

   On Windows, if Kate doesn't find the program, use the full path:
   `"command": ["C:\\Users\\YOU\\AppData\\Local\\Programs\\xxmi\\xxmi-lsp.exe", "--stdio"]`.

4. Restart Kate and open a mod `.ini`. If it opens as "INI Files", switch it to **Migoto** in
   Tools → Mode.

### Claude Code

```bash
claude plugin marketplace add xxmi-lang/xxmi-lang
claude plugin install xxmi@xxmi-lang
```

The plugin runs the `xxmi-lsp` on your PATH, so install it first (above) and start Claude Code
from a new terminal afterwards. After each edit to an `.ini` file, the agent sees the errors.

### Other editors

Any editor with an LSP client (Neovim, Helix, Zed, Sublime LSP, …) can run the server: start
`xxmi-lsp --stdio` for `.ini` files, with the folder that has `d3dx.ini` as the root. We
haven't tested configurations for these yet.

## Updating and uninstalling

- **Update:** run the same download commands again; they replace the files.
  `xxmi-lsp --version` shows what you have.
- **Uninstall:** delete the two files (`~/.local/bin/xxmi-lsp` and `~/.local/bin/xxmi`, or the
  `%LOCALAPPDATA%\Programs\xxmi` folder on Windows). Uninstall the VS Code extension from the
  Extensions view.

## Troubleshooting

- **`xxmi-lsp: command not found` (or "is not recognized").** The folder isn't on your PATH yet:
  repeat the PATH step and open a new terminal. Editors started before you changed the PATH
  keep the old one, so restart them too (on Linux, log out and in).
- **`Illegal instruction` on Linux or Windows x64.** The x64 builds need a CPU from roughly
  2013 or newer (AVX2). Please open an issue; a build for older CPUs can be added.
- **Kate shows no errors.** Check that the file is in **Migoto** mode (Tools → Mode) and that
  `xxmi-lsp --version` works in a terminal. Kate's "Output" tool view (LSP Client tab) shows
  server messages.
- **"Unresolved … CommandList\ZZMI\…" errors.** Open the ZZMI folder (with `d3dx.ini`) rather
  than only the mod. The tools then read your installed libraries instead of the copy bundled with
  the release, which may be older than your ZZMI.
