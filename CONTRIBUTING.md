# Contributing

Simplified Gitflow. Nothing reaches `main` or `develop` without a PR.

## Branches

| Branch | Purpose | Branches off | Merges back into |
|---|---|---|---|
| `main` | Stable version; each version carries the tag `vX.Y.Z` | — | — |
| `develop` | Integration; default branch | `main` (at the start) | `main`, by PR |
| `feature/<topic>` | New feature | `develop` | `develop`, by PR |
| `fix/<topic>` | Fix | `develop` | `develop`, by PR |
| `release/X.Y.Z` | Version bump | `develop` | `develop`, by PR |

## Flow

1. `git switch develop && git pull`, then `git switch -c feature/<topic>`.
2. Small commits, with `scripts/check.sh` green before each one.
3. `git push -u origin feature/<topic>` and `gh pr create --base develop`.
4. It is merged once CI is green on Mac and Windows (**squash** is fine here). The branch is deleted automatically.
5. New version: a `release/X.Y.Z` branch off `develop` that bumps `version` in `Cargo.toml` (and `Cargo.lock` with `cargo build`), a PR to `develop`, and then a `develop` → `main` PR merged with a **merge commit** (with squash the two branches diverge). The `vX.Y.Z` tag goes on that merge in `main` (`git tag -a vX.Y.Z -m "CuantoQuedapp X.Y.Z"` and `git push origin vX.Y.Z`). Pushing the tag starts the **Release** workflow, which builds the macOS and Windows zips and creates a **draft** release with them and their `.sha256` files. Review the draft on GitHub, write "What's new" and publish it.

## Code conventions

- Code, comments and docs are in English.
- Interface texts go in `locales/app.yml`, in English and Spanish, and are looked up with `t!(…, locale = lang.code())`. A test fails if a key lacks a language or is unused.
- No `unwrap`/`panic` when reading data: errors are values with a clear message. Problems shown to the user are values too (like `Notice` or `CliError`), put into words only when shown.
- The current time and the language are passed as parameters, so tests are deterministic.
- Tests use hand-written fixtures in temporary folders, never your real `~/.codex`, and nothing ever prints conversation content: only limit fields.

## Local hook

To avoid pushing to `main` or `develop` by mistake, enable it once per clone:

```
git config core.hooksPath scripts/git-hooks
```

The `scripts/git-hooks/pre-push` hook rejects any push to `main` or `develop`, including deleting them; tags and other branches go through.

## GitHub Actions

- **CI** (`.github/workflows/ci.yml`): tests, clippy and fmt on macOS and Windows on every PR to `develop` or `main` and on every push to `develop`. It can be started by hand (`gh workflow run CI --ref develop`).
- **Audit** (`audit.yml`): `cargo audit` every Monday and on every PR or push to `develop` that changes `Cargo.lock` or `Cargo.toml`; it only fails on real vulnerabilities.
- **Release** (`release.yml`): on a `vX.Y.Z` tag, checks that the tag matches `Cargo.toml` and is on `main`, builds `CuantoQuedapp-X.Y.Z-macos-arm64.zip` (`scripts/package-macos.sh --zip`) and `CuantoQuedapp-X.Y.Z-windows-x64.zip` (`scripts/build-windows.ps1`) from scratch, checks them (no build paths inside, signature intact after unzipping, no Visual C++ runtime needed) and creates the draft release. Both builds need `cargo-about`, which writes `THIRD-PARTY-LICENSES.html`; a dependency with a license not in `about.toml` stops the build until it is reviewed and added. A PR that changes the packaging runs the same builds and checks without releasing anything, and keeps the zips as run artifacts for 14 days.
- Actions are pinned to commit hashes (the tag is in a comment next to each); Dependabot updates them.
- **Dependabot** (`dependabot.yml`): one grouped monthly PR to `develop` for Cargo and another for the Actions.
