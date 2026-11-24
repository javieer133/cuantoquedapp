# Windows: build, test and what to report

Status (November 2026): it builds and passes clippy for Windows, and it has run on Windows 11: the CLI with `--live` shows Codex and Claude, and the tray reads Claude. The rest of the checklist is still unchecked.

The app follows the Windows display language (English or Spanish); you can force one in Settings > Language, or with `CUANTOQUEDAPP_LANG=en` / `es`. The texts below are the English ones.

## Running it

What a computer needs to run `cuantoquedapp.exe` (building it needs more, see the next section):

- **Windows 10 or 11, 64-bit.** The `.exe` is self-contained: the C runtime is inside it, so the Visual C++ Redistributable is not needed, and neither are the Visual Studio Build Tools.
- **WebView2**, for the panel: it comes with Windows 11, and with Microsoft Edge on Windows 10. Without it the app still works, with a text menu instead of the panel.
- **The first run:** the `.exe` is not signed, so SmartScreen may show "Windows protected your PC". Click **More info → Run anyway**.
- **A fixed folder** before turning on "Start at login", for example `%LOCALAPPDATA%\Programs\CuantoQuedapp`: Windows remembers the path of the `.exe`, and moving it later (say, out of Downloads) breaks the start at login.
- **Codex** and/or **Claude Code**, as on the Mac (see the [README](../README.md#requirements)).

## Requirements to build

1. **Rust with the MSVC toolchain**: install it from <https://rustup.rs> (pick the default, `x86_64-pc-windows-msvc`).
2. **Visual Studio Build Tools** with the "Desktop development with C++" workload. It includes the linker and the Windows SDK; `winresource` also needs the SDK to embed the `.exe` icon.
3. **Codex** installed and used at least once, so that sessions exist.
4. Optional, to see Claude: **Claude Code** installed, signed in (`claude auth login`) and with a subscription.

`.cargo/config.toml` (and `scripts\build-windows.ps1`, because a `RUSTFLAGS` variable replaces that file's setting) builds with `-C target-feature=+crt-static`, which puts the C runtime inside the `.exe`.

## Build

From the project folder, in PowerShell:

```
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```

It leaves `cuantoquedapp.exe` (tray) and `cuantoquedapp-cli.exe` (terminal) in `dist\windows\`, and `THIRD-PARTY-LICENSES.html` when [cargo-about](https://github.com/EmbarkStudios/cargo-about) is installed (`cargo install cargo-about --locked --features cli`): optional for testing, required before sharing the `.exe`. If it fails, copy the whole message. If only the embedded icon fails, the build goes on (it is a warning): the app works without that icon.

## If it fails: `linker link.exe not found`

Rust on Windows needs Microsoft's linker, which does not come with Rust: the C++ tools are missing.

1. Diagnose in PowerShell:
   ```
   rustup show active-toolchain
   & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -latest -property installationPath
   ```
   The first line must say `stable-x86_64-pc-windows-msvc`. If the second prints no path (or `vswhere.exe` does not exist), the C++ component is not installed.
2. Install the Visual Studio Build Tools (<https://visualstudio.microsoft.com/visual-cpp-build-tools/>) with the "Desktop development with C++" workload (it includes MSVC and the Windows SDK). If they are already installed, open the Visual Studio Installer, choose "Modify" and check that workload.
3. Close the terminal, open a new one and run `scripts\build-windows.ps1` again. No developer console is needed: Rust finds the linker with `vswhere`.

## What to check (tick each item)

1. **Data:** `dist\windows\cuantoquedapp-cli.exe` prints the Codex limits. If it says "Codex not found on this computer", check the path (see below).
2. **Tray:** opening `cuantoquedapp.exe` shows no console window and does show a square icon in the tray (it may be in the "^" overflow). It starts as a grey "?" and turns into a colored number once it reads the data.
3. **Readability:** can you read the number at normal size? The digits are 3×5 pixels on a 16×16 icon at 100 % scaling. If they are hard to read, the design has to change (one bigger digit, or color only).
4. **Tooltip:** on hover it shows something like `Codex: 5 hours 46% · Week 25%`.
5. **Panel and menu:** left click opens a panel above the tray with colored bars (it closes when you click outside and does not appear in the taskbar); right click shows "Start at login" and "Quit". If the panel does not open, check that WebView2 is installed (it ships with Windows 11) and report it: without it the app shows the text menu.
6. **Single instance:** opening the `.exe` a second time does not create a second icon.
7. **Start at login:** turning on "Start at login" creates the `cuantoquedapp` value in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (it also shows up in Task Manager > Startup). Turning it off removes it. Signing out and back in must open the app.
8. **Quit** closes the program and removes the icon.
9. **File icon:** in Explorer, `cuantoquedapp.exe` shows the green gauge.
10. **Claude (live):** `dist\windows\cuantoquedapp-cli.exe --live` must show a "Claude" section with 5 hours and Week, matching `/usage` inside Claude Code. If it does not, try `claude -p "/usage" --no-session-persistence` in PowerShell (it must print "Current session… Current week…"). The app looks for `claude.exe` or `claude.cmd` in `CUANTOQUEDAPP_CLAUDE` (an absolute path), `%USERPROFILE%\.local\bin`, the `PATH`, `%APPDATA%\npm` and the VS Code extension. No console may flash when the app opens.
11. **Claude in the tray:** the tooltip shows one line per tool ("Codex: …" and "Claude: …"); the icon number is Codex's (or Claude's if there is no Codex data). The panel shows both cards with their icons.
12. **Self-contained:** in a "Developer PowerShell for VS", `dumpbin /dependents dist\windows\cuantoquedapp.exe` must not list `VCRUNTIME140.dll` (only Windows system DLLs such as `KERNEL32.dll`). Best of all: copy `cuantoquedapp.exe` alone to a Windows without Visual Studio or the Visual C++ Redistributable (for example a fresh virtual machine) and check that it opens.

## Manual refresh and settings

- "Refresh now" (panel footer and right-click menu) forces an immediate read of Codex and Claude and shows "Refreshing…" meanwhile.
- The "Show" and "Alert above" submenus change what is shown; they are saved in `%APPDATA%\cuantoquedapp\config.json`. The tooltip puts `⚠ ` in front of the tool in alert.
- If a read stops answering, the app replaces it by itself after 5 minutes (logged in `debug.log` with `CUANTOQUEDAPP_DEBUG=1`).

## If left click does not open the panel

The panel is a web window (WebView2) with no console to show errors. To log what happens, in PowerShell:

```
$env:CUANTOQUEDAPP_DEBUG = 1
.\dist\windows\cuantoquedapp.exe
```

Left-click the icon, quit with right click > "Quit" and paste the content of `%LOCALAPPDATA%\cuantoquedapp\debug.log` (only geometry and event names, no data). It tells whether the panel was created, whether the click arrived, where it opened and whether it closed on losing focus, and, for Claude, whether `claude` was found, whether it ran and whether its output was understood (only the "Current…" lines, with percentages and times). Delete the file when you are done.

## If Claude does not show up

Try these in PowerShell, in this order, and see where it fails:

1. `where.exe claude`: must print a path (`claude.exe` or `claude.cmd`). If not, Claude Code is not installed or not on the `PATH`.
2. `claude -p "/usage" --no-session-persistence` (in your terminal; if it says "Total cost" instead of "Current session", Claude Code is using an API key through the `ANTHROPIC_API_KEY` variable: the app already removes it when launching `claude`, but this manual command does not; try it with `Remove-Item Env:ANTHROPIC_API_KEY` in that window): it must print "Current session: … Current week …". If it asks you to sign in, run `claude auth login` (the Mac's session does not carry over to another computer).
3. `.\dist\windows\cuantoquedapp-cli.exe --live`: must show the "Claude" section.
4. With `CUANTOQUEDAPP_DEBUG=1` (above), `debug.log` tells which step failed inside the app.

## Claude limitations on Windows

- Only the main source (`claude -p "/usage"`); the `statusLine` fallback (`cuantoquedapp-statusline`, `scripts/claude-statusline.sh`) is for Mac and Linux only.
- Reset times are read in the Windows time zone.
- If the `/usage` text changes or Claude Code is not signed in, Claude simply does not show up (the app does not make up numbers).

## Codex sessions path (unverified assumption)

It looks in this order: `%CODEX_HOME%\sessions`, `%USERPROFILE%\.codex\sessions`. On the Mac it was confirmed that Codex supports `CODEX_HOME`; that Windows uses `%USERPROFILE%\.codex` is an assumption. If Codex stores its sessions elsewhere, pass the folder by hand:

```
dist\windows\cuantoquedapp-cli.exe --folder "C:\path\to\sessions"
```

## Uninstall

1. Icon menu → uncheck "Start at login" (or delete the `cuantoquedapp` value from the `Run` key).
2. Quit and delete the `dist\windows` folder.
3. Optional: delete `%LOCALAPPDATA%\cuantoquedapp` (it only holds the single-instance lock file).

## What to report

Open an issue with the result of each checklist item (OK / fails + message or screenshot). That is enough to fix what is needed.
