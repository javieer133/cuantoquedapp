#!/bin/sh
# Every check before a commit: tests, clippy (Mac and the Windows target) and format.
# Prints one line per check; on a failure, the end of its output and exit 1.
set -eu
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi

run() {
  name=$1
  shift
  if output=$("$@" 2>&1); then
    last_output=$output
    echo "ok: $name"
  else
    echo "$output" | tail -n 40
    echo "FAILED: $name"
    exit 1
  fi
}

run "cargo test" cargo test
passed=$(echo "$last_output" | awk '/^test result:/ {s += $4} END {print s + 0}')
echo "   $passed tests passing"
run "clippy (Mac)" cargo clippy --all-targets -- -D warnings
if rustup target list --installed 2>/dev/null | grep -q '^x86_64-pc-windows-msvc$'; then
  run "clippy (Windows)" cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
else
  echo "skipped: clippy (Windows), the x86_64-pc-windows-msvc target is missing"
fi
run "fmt" cargo fmt --check
