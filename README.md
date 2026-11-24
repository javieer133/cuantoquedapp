# CuantoQuedapp

**English** · [Español](README.es.md)

*The name comes from the Spanish "¿cuánto queda?": "how much is left?".*

A menu bar icon on the Mac (and a tray icon on Windows) that shows how much of your **Codex** and **Claude Code** usage limits you have used: the 5-hour window and the weekly one, with the time each one resets.

The app speaks English and Spanish: it follows the system language, and you can choose one in Settings.

```
[>_] 54%  [✱] 45%              ← in the menu bar: Codex and Claude (5-hour window)

Left click → panel with colored bars:

  [>_] Codex  plus                 data from 0 min ago
  5 hours                                             54%
  ████████████░░░░░░░░░░
  resets in 3 h 40 min (Sat Nov 21 00:23)
  Week                                                27%
  █████░░░░░░░░░░░░░░░░░
  resets in 6 d 11 h (Thu Nov 26 20:46)

  [✱] Claude                       data from 0 min ago
  ...
  ─────────────────────────────
  Refresh now
  Settings                                             ›
  Quit
```

CuantoQuedapp itself sends nothing to any server: it has no network code of its own, never stores or reads passwords or tokens, and only parses the limit fields (percentages and times), never the content of your conversations. To get live data it runs your own Codex and Claude Code on this computer, and they talk to their services as they always do, with their own login and settings.

CuantoQuedapp is an independent project, not affiliated with or endorsed by OpenAI or Anthropic. Codex and Claude are trademarks of their owners.

## Requirements

| To see… | You need |
|---|---|
| **Codex** | Codex installed and used at least once with your ChatGPT account |
| **Claude** | Claude Code installed and signed in with your **subscription** (Pro, Max…): `claude auth login` |
| **The Mac** | macOS 11 or later on **Apple Silicon** (M1 or newer). On an Intel Mac you have to build it (see below) |

It works with just one of the two tools: only the one you have shows up.

## Install on the Mac

1. Download `CuantoQuedapp-<version>-macos-arm64.zip` from [Releases](https://github.com/javieer133/cuantoquedapp/releases), or build it yourself (see "Build from source").
2. Double-click the zip to unpack it and drag **CuantoQuedapp** to the **Applications** folder.
3. Open it. The first time, macOS blocks it because it is not signed with a paid Apple certificate (it is signed locally). To open it once:
   - **Option A:** in **System Settings → Privacy & Security**, scroll down to the notice about "CuantoQuedapp" and click **Open Anyway**.
   - **Option B (Terminal):** `xattr -dr com.apple.quarantine "/Applications/CuantoQuedapp.app"` and open it again.
4. It appears in the menu bar (it has no Dock icon). Wait a couple of seconds while it reads the data.

To check that the file was not altered, compare its fingerprint with the `.sha256` file published next to it: `shasum -a 256 CuantoQuedapp-*-macos-arm64.zip`.

## How to use it

- **Left click** on the icon: opens the panel with each tool's bars and the time left until each window resets. It closes when you click outside or press Esc. At the bottom: "Refresh now", "Settings" and "Quit".
- **Settings** (in the panel; the right-click menu also has "Show" and "Alert above"): which windows and tools to show, which number goes in the menu bar, an alert above 50, 75 or 90 %, light or dark theme (or the system's), the language (the system's, Español or English) and "Start at login".
- **Start at login**: makes the app start by itself when you log in. Only available when the app is installed in Applications. macOS may show a "background item" notice: that is normal.
- Colors warn about usage: green below 60 %, amber up to 85 %, red above.
- The alert: in the menu bar, a tool's number turns red when one of its shown windows (5 hours or week) reaches the chosen level; on Windows the tooltip puts `⚠` in front of it.
- The app refreshes by itself: Codex every minute, Claude every 5 minutes at most, and both right after a window resets.
- Opening the app a second time does not create a second icon.

### What the menu bar text means

| Text | Meaning |
|---|---|
| `[>_] 54%` | You have used 54 % of the Codex 5-hour window |
| `[✱] 45%` | The same for Claude |
| `… · old data` | The latest reading is more than an hour old |
| `… · warning` | Something in the panel needs a look (for example, a format that could not be understood) |
| `[✱] ↻` | That window just reset: the old number is hidden and the app is already asking for the new one (it takes a few seconds) |
| `?% · outdated` | The data is dated in the future: check the computer's clock |
| `Codex ?% · not reading` | The internal reader stopped; restart the app |
| `Codex ?%` | No data yet |

## Troubleshooting

| Symptom | What to do |
|---|---|
| macOS says the app is damaged or cannot be opened | It is the first-run block: see steps 3A or 3B of "Install" |
| Only Codex shows up, not Claude | In Terminal, check that `claude -p "/usage" --no-session-persistence` prints "Current session…". If it prints costs ("Total cost…"), Claude Code is using an **API key** instead of your subscription: run `claude auth login` with your subscription account. The app already ignores the `ANTHROPIC_API_KEY` variable when it launches `claude`, but the subscription session has to exist |
| Only Claude shows up, not Codex | Open Codex once and use it, so it creates its data. If it lives somewhere else, set `CUANTOQUEDAPP_CODEX` to the absolute path of the executable |
| The number looks behind | The app asks for Codex data every minute and Claude data every 5; if the title says "old data", Codex or Claude did not answer |
| I want to see what is going on | Quit it and open it from Terminal with `CUANTOQUEDAPP_DEBUG=1 "/Applications/CuantoQuedapp.app/Contents/MacOS/cuantoquedapp"`: it writes `debug.log` (only events and geometry, no data) in `~/Library/Caches/cuantoquedapp/` |

## Uninstall

1. In the menu, uncheck "Start at login" and choose "Quit".
2. Delete `/Applications/CuantoQuedapp.app`.
3. Optional: delete the `~/Library/Caches/cuantoquedapp` folder (it holds a lock file, the last Claude reading from the fallback and, if you turned it on, the debug log).
4. If you installed the Claude fallback: `scripts/claude-statusline.sh uninstall`.

## Windows

There is no ready-made download yet: you have to build it. The `.exe` is self-contained (no Visual C++ Redistributable needed); what Windows needs to run it, and the first-run SmartScreen notice, are in [docs/windows.md](docs/windows.md#running-it). The step-by-step guide (requirements, build, testing, uninstalling and what to do if something fails) is in [docs/windows.md](docs/windows.md). On Windows the tray shows no text: the Codex percentage is drawn inside the icon (or Claude's if there is no Codex), and the details show up on hover and in the left-click panel.

## Build from source

Useful if you have an Intel Mac or want to change it. You need [Rust](https://rustup.rs), Git and [cargo-about](https://github.com/EmbarkStudios/cargo-about) (`cargo install cargo-about --locked --features cli`), which writes the third-party license notices into the app:

```
git clone https://github.com/javieer133/cuantoquedapp.git
cd cuantoquedapp
scripts/package-macos.sh              # creates dist/CuantoQuedapp.app
scripts/package-macos.sh --install    # also copies it to /Applications
scripts/package-macos.sh --zip        # also creates the zip and its fingerprint for sharing
```

The flags can be combined (`--install --zip`). The app is signed locally ("ad hoc"), so it runs without warnings on the Mac that built it. Handing it to other Macs without the first-run notice would need an Apple Developer ID signature and notarization (paid, not included).

## In the terminal

```
cargo run --bin cuantoquedapp-cli                       # the same data, from Codex files only
cargo run --bin cuantoquedapp-cli -- --live             # live Codex and Claude, like the icon
cargo run --bin cuantoquedapp-cli -- --folder PATH       # another Codex sessions folder
cargo run --bin cuantoquedapp-cli -- --now 2026-11-20T12:00:00Z   # fixed time (tests)
```

## How it gets the data

- **Codex**: reads the limits Codex already stores in `~/.codex/sessions` (or `$CODEX_HOME/sessions`) and also asks Codex itself for them live (`codex app-server`, a local process the app starts and stops). If the native executable is not found, it uses only the files.
- **Claude**: runs `claude -p "/usage" --no-session-persistence`, a local Claude Code command (it does not call the model, costs nothing and leaves no sessions behind). It uses Claude Code's own login: the app never sees a token. When it launches `claude` it removes `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN`, only for that process, because with them Claude Code shows API costs instead of your subscription limits.
- **Optional Claude fallback (Mac and Linux)**: `scripts/claude-statusline.sh install` chains a small helper in front of Claude Code's `statusLine` to save its limits while a terminal session is open. It backs up `~/.claude/settings.json` and is undone with `uninstall` (`status` shows whether it is installed). It is only used when `/usage` does not answer, and it may lag behind.
- Environment variables: `CUANTOQUEDAPP_CODEX` and `CUANTOQUEDAPP_CLAUDE` (absolute path of the executable; relative paths, and relative `PATH` entries, are ignored), `CUANTOQUEDAPP_DEBUG` (turns on the log), `CUANTOQUEDAPP_LANG` (`en` or `es`: forces the language, over the setting and the system).

## Known limits

- It depends on formats that are **not documented** (Codex's files, `codex app-server` and the text of Claude Code's `/usage`) and may change with an update. If that happens, the app says so ("Could not read the format…") or stops showing that tool; it never makes up a number.
- File data is this computer's; the live data also reflects what you used from another computer or the web.
- Claude's reset times are read in the system time zone.
- The menu bar icons are original drawings, not the official OpenAI or Anthropic logos.
- Windows builds but has had little testing; see [docs/windows.md](docs/windows.md).

## Development

```
cargo test                                            # tests (hand-written fixtures, never ~/.codex)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo check --target x86_64-pc-windows-msvc           # check Windows from the Mac (rustup target add ...)
cargo test real_launch_agent -- --ignored            # real start-at-login test (touches ~/Library/LaunchAgents)
```

Interface texts live in `locales/app.yml`, English and Spanish side by side; a test fails if a text lacks either language. Helper examples in `examples/`: `merged_report` (what the icon would show), `panel_html` (the panel page), `show_report`, `measure`, `view_icon`, `generate_icon`.

`src/lib.rs` holds the logic without an interface, `src/tray/` and `src/panel*.rs` are the interface, and `src/bin/` has the programs. How to contribute (branches, checks, releases): [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

The licenses of the open source crates CuantoQuedapp is built with are listed in `THIRD-PARTY-LICENSES.html`, inside the app (`CuantoQuedapp.app/Contents/Resources/`) and next to the Windows `.exe`; `about.toml` lists the licenses accepted.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
