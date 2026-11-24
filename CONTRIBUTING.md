# Contributing

Thanks for helping. Bug reports, ideas and pull requests are welcome, in English or Spanish.

## Before you start

- **Bugs and ideas:** open an [issue](https://github.com/javieer133/cuantoquedapp/issues/new/choose) with the form.
- **Bigger changes** (a new feature, a new dependency, a change in how data is read): open an issue first, so we agree on the approach before you put in the work. Small fixes can go straight to a PR.
- **Security problems:** do not open an issue. Report them privately through [private vulnerability reporting](https://github.com/javieer133/cuantoquedapp/security/advisories/new).

## What you need

- [Rust](https://rustup.rs), current stable (the project uses the 2024 edition; CI always builds with the latest stable).
- macOS on Apple Silicon or Windows 10/11. On Windows, the Visual Studio Build Tools as well (see [docs/windows.md](docs/windows.md#requirements-to-build)) and Git Bash to run `scripts/check.sh`.
- Optional: the Windows target on the Mac, so `scripts/check.sh` also runs clippy for Windows: `rustup target add x86_64-pc-windows-msvc`. Without it the check is skipped, and CI runs it anyway.
- Only to build the packaged app: [cargo-about](https://github.com/EmbarkStudios/cargo-about) (`cargo install cargo-about --locked --features cli`).
- To see real data: Codex and/or Claude Code installed and signed in (see [Requirements](README.md#requirements)).

## Making a change

1. Fork the repository on GitHub and clone your fork.
2. Branch off `develop` (the default branch; `main` only receives releases):
   ```
   git switch develop
   git switch -c fix/<topic>        # or feature/<topic>
   ```
3. Run it from source while you work:
   ```
   cargo run                                  # the menu bar / tray icon
   cargo run --bin cuantoquedapp-cli          # the limits in the terminal
   cargo run --bin cuantoquedapp-cli -- --live  # asks Codex and Claude for live data
   ```
   More helpers (the panel page, the icon) are under "Development" in the [README](README.md#development).
4. Keep commits small, with `scripts/check.sh` green before each one. It runs the tests, clippy (macOS and, if installed, the Windows target) and `cargo fmt --check`.
5. Push to your fork and open a pull request **against `develop`**. The PR template has a short checklist.
6. CI runs tests and clippy on macOS and Windows. The first time you contribute, a maintainer approves the run. A maintainer merges once CI is green.

## Code conventions

- Code, comments, docs and commit messages are in English. Commit subjects are short and say what changes ("Keep a reset tool in the menu bar").
- Interface texts go in `locales/app.yml`, in English and Spanish, and are looked up with `t!(…, locale = lang.code())`. A test fails if a key lacks a language or is unused. If you do not speak one of the two languages, write the text in the one you know and say so in the PR.
- If the README changes, change both `README.md` and `README.es.md`.
- No `unwrap`/`panic` when reading data: errors are values with a clear message. Problems shown to the user are values too (like `Notice` or `CliError`), put into words only when shown.
- The current time and the language are passed as parameters, so tests are deterministic.
- Tests use hand-written fixtures in temporary folders, never your real `~/.codex`, and nothing ever prints conversation content: only limit fields.
- New dependencies need a reason in the PR: every one ends up in the app and in `THIRD-PARTY-LICENSES.html`.

## License of contributions

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in CuantoQuedapp is dual licensed as `MIT OR Apache-2.0`, like the rest of the project (see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)), without any additional terms or conditions.

---

## For maintainers

### Branches

Simplified Gitflow. Nothing reaches `main` or `develop` without a PR; both are protected (PR required, CI green on macOS and Windows, no force pushes, no deletion).

| Branch | Purpose | Branches off | Merges back into |
|---|---|---|---|
| `main` | Stable version; each version carries the tag `vX.Y.Z` | — | — |
| `develop` | Integration; default branch | `main` (at the start) | `main`, by PR |
| `feature/<topic>` | New feature | `develop` | `develop`, by PR |
| `fix/<topic>` | Fix | `develop` | `develop`, by PR |
| `release/X.Y.Z` | Version bump | `develop` | `develop`, by PR |

PRs to `develop` are merged with **squash**; the branch is deleted automatically.

### Releases

1. A `release/X.Y.Z` branch off `develop` that bumps `version` in `Cargo.toml` (and `Cargo.lock` with `cargo build`), and a PR to `develop`.
2. A `develop` → `main` PR merged with a **merge commit** (with squash the two branches diverge).
3. The `vX.Y.Z` tag on that merge in `main`: `git tag -a vX.Y.Z -m "CuantoQuedapp X.Y.Z"` and `git push origin vX.Y.Z`.
4. Pushing the tag starts the **Release** workflow, which builds the macOS and Windows zips and creates a **draft** release with them and their `.sha256` files. Review the draft on GitHub, write "What's new" and publish it.

### Local hook

To avoid pushing to `main` or `develop` by mistake, enable it once per clone:

```
git config core.hooksPath scripts/git-hooks
```

The `scripts/git-hooks/pre-push` hook rejects any push to `main` or `develop`, including deleting them; tags and other branches go through.

### GitHub Actions

- **CI** (`.github/workflows/ci.yml`): tests, clippy and fmt on macOS and Windows on every PR to `develop` or `main` and on every push to `develop`. It can be started by hand (`gh workflow run CI --ref develop`).
- **Audit** (`audit.yml`): `cargo audit` every Monday and on every PR or push to `develop` that changes `Cargo.lock` or `Cargo.toml`; it only fails on real vulnerabilities.
- **Release** (`release.yml`): on a `vX.Y.Z` tag, checks that the tag matches `Cargo.toml` and is on `main`, builds `CuantoQuedapp-X.Y.Z-macos-arm64.zip` (`scripts/package-macos.sh --zip`) and `CuantoQuedapp-X.Y.Z-windows-x64.zip` (`scripts/build-windows.ps1`) from scratch, checks them (no build paths inside, signature intact after unzipping, no Visual C++ runtime needed) and creates the draft release. Both builds need `cargo-about`, which writes `THIRD-PARTY-LICENSES.html`; a dependency with a license not in `about.toml` stops the build until it is reviewed and added. A PR that changes the packaging runs the same builds and checks without releasing anything, and keeps the zips as run artifacts for 14 days.
- Actions are pinned to commit hashes (the tag is in a comment next to each); Dependabot updates them.
- **Dependabot** (`dependabot.yml`): one grouped monthly PR to `develop` for Cargo and another for the Actions; security updates as soon as an advisory appears.
