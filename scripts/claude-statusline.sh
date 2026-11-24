#!/usr/bin/env bash
# Chains the Claude Code status line through cuantoquedapp-statusline so the app can read
# Claude's limits. The command you had keeps running exactly as before.
#
#   scripts/claude-statusline.sh install     edit ~/.claude/settings.json (with a backup)
#   scripts/claude-statusline.sh uninstall   put your previous command back
#   scripts/claude-statusline.sh status      say what is installed
#   scripts/claude-statusline.sh refresh [N] re-run the status line every N seconds (default 60)
#                                            so limits stay fresh without new messages
#   scripts/claude-statusline.sh refresh off remove the periodic refresh again
#
# Environment (for tests): CLAUDE_SETTINGS (default ~/.claude/settings.json) and
# CUANTOQUEDAPP_STATUSLINE (default: the helper inside /Applications/CuantoQuedapp.app).
set -euo pipefail

SETTINGS="${CLAUDE_SETTINGS:-$HOME/.claude/settings.json}"
HELPER="${CUANTOQUEDAPP_STATUSLINE:-/Applications/CuantoQuedapp.app/Contents/MacOS/cuantoquedapp-statusline}"
CHAIN_DIR="$HOME/.claude/cuantoquedapp"

ACTION="${1:-}" ARG="${2:-}" SETTINGS="$SETTINGS" HELPER="$HELPER" CHAIN_DIR="$CHAIN_DIR" exec python3 - <<'PY'
import json, os, shlex, shutil, sys, tempfile, time

action = os.environ["ACTION"]
settings_path = os.environ["SETTINGS"]
helper = os.environ["HELPER"]
chain_dir = os.environ["CHAIN_DIR"]
chain_file = os.path.join(chain_dir, "chain.sh")
# Present only when the refresh interval was added by this script (so uninstall removes it).
refresh_flag = os.path.join(chain_dir, "refresh-added")
arg = os.environ["ARG"]
MARKER = "cuantoquedapp-statusline"


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def load():
    try:
        with open(settings_path, encoding="utf-8") as handle:
            return json.load(handle)
    except FileNotFoundError:
        return {}
    except (OSError, ValueError) as error:
        fail(f"Could not read {settings_path} ({type(error).__name__}); nothing was changed.")


def save(settings):
    # A unique temporary file in the same folder (never a shared name), with the permissions
    # the file already has, then an atomic replace.
    folder = os.path.dirname(os.path.abspath(settings_path))
    handle, temp = tempfile.mkstemp(prefix=".settings.", suffix=".tmp", dir=folder)
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as out:
            json.dump(settings, out, indent=2, ensure_ascii=False)
            out.write("\n")
        try:
            os.chmod(temp, os.stat(settings_path).st_mode & 0o777)
        except FileNotFoundError:
            os.chmod(temp, 0o600)
        os.replace(temp, settings_path)
    except BaseException:
        if os.path.exists(temp):
            os.remove(temp)
        raise


def write_chain(text):
    """chain.sh is run as a shell command: a private folder, never written through a link,
    and replaced atomically by a new 0600 file (never an existing file or link)."""
    os.makedirs(chain_dir, mode=0o700, exist_ok=True)
    if os.path.islink(chain_dir):
        fail(f"{chain_dir} is a link; nothing was changed.")
    os.chmod(chain_dir, 0o700)
    temp = os.path.join(chain_dir, f".chain.sh.{os.getpid()}.tmp")
    try:
        fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
        os.replace(temp, chain_file)
    except BaseException:
        if os.path.exists(temp):
            os.remove(temp)
        raise


def backup_file():
    """Copy of the current file under a name that does not exist yet (nothing is overwritten)."""
    base = settings_path + ".cuantoquedapp.bak"
    for suffix in [""] + ["." + time.strftime("%Y%m%d%H%M%S") + (f"-{n}" if n else "") for n in range(100)]:
        candidate = base + suffix
        if not os.path.exists(candidate):
            shutil.copy2(settings_path, candidate)
            return candidate
    fail("Could not create a new backup copy; nothing was changed.")


def current_command(settings):
    line = settings.get("statusLine")
    if isinstance(line, dict) and line.get("type") == "command":
        return line.get("command")
    return None


settings = load()
command = current_command(settings)
installed = isinstance(command, str) and MARKER in command

if action == "status":
    if installed:
        print("Installed: the statusLine goes through cuantoquedapp-statusline.")
    else:
        print("Not installed.")
elif action == "install":
    if installed:
        print("Already installed; nothing was changed.")
        sys.exit(0)
    if not (os.path.isfile(helper) and os.access(helper, os.X_OK)):
        fail(f"Helper executable not found: {helper}\nInstall the app with scripts/package-macos.sh --install.")
    existing = settings.get("statusLine")
    if existing is not None and command is None:
        fail("Your statusLine is not of type \"command\", so it cannot be chained. Nothing was changed.")
    if os.path.exists(settings_path):
        print(f"Backup copy: {backup_file()}")
    write_chain(command or "")
    # If the helper disappears (app removed), the previous command runs as it always did.
    quoted = shlex.quote(helper)  # one literal word whatever characters the path has
    wrapper = f'# {MARKER}\nif [ -x {quoted} ]; then exec {quoted}; fi\n{command or ""}'
    line = dict(existing) if isinstance(existing, dict) else {}
    line.update({"type": "command", "command": wrapper})
    settings["statusLine"] = line
    save(settings)
    print("Installed. Claude Code applies it in the next session (or when the open ones are reopened).")
elif action == "refresh":
    if not installed:
        fail("Run install first: refreshing only makes sense with the helper installed.")
    line = settings["statusLine"]
    if arg == "off":
        if os.path.exists(refresh_flag):
            line.pop("refreshInterval", None)
            os.remove(refresh_flag)
            save(settings)
            print("Periodic refresh removed.")
        else:
            print("The refresh was not set by this script; it was left alone.")
        sys.exit(0)
    try:
        seconds = int(arg) if arg else 60
    except ValueError:
        fail("The interval must be a number of seconds (at least 5), or \"off\".")
    if seconds < 5:
        fail("The interval must be a number of seconds (at least 5), or \"off\".")
    if "refreshInterval" in line and not os.path.exists(refresh_flag):
        fail(f"refreshInterval={line['refreshInterval']} was already set; it was left as is.")
    print(f"Backup copy: {backup_file()}")
    line["refreshInterval"] = seconds
    open(refresh_flag, "w").close()
    save(settings)
    print(f"Done: the statusLine runs again every {seconds} s while a session is open.")
elif action == "uninstall":
    if not installed:
        print("Not installed; nothing was changed.")
        sys.exit(0)
    try:
        with open(chain_file, encoding="utf-8") as handle:
            previous = handle.read()
    except OSError:
        fail(f"{chain_file} not found; restore it by hand from the .cuantoquedapp.bak copy.")
    if os.path.exists(refresh_flag):
        settings["statusLine"].pop("refreshInterval", None)
        os.remove(refresh_flag)
    if previous.strip():
        settings["statusLine"]["command"] = previous
    else:
        del settings["statusLine"]
    save(settings)
    os.remove(chain_file)
    print("Uninstalled: your previous command is back in place.")
else:
    fail("Usage: claude-statusline.sh install | uninstall | status | refresh [seconds|off]")
PY
