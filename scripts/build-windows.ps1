# Compila cuantoquedapp para Windows y deja los .exe en dist\windows.
#
# Uso, desde una terminal de PowerShell:
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
#
# Requisitos: ver docs\windows.md (Rust con el toolchain MSVC y las Build Tools de Visual Studio).

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo not found. Install Rust from https://rustup.rs and open a new terminal."
}

# Keep this machine's user name and folders out of the binaries (panic messages embed paths),
# and put the C runtime inside the .exe (see .cargo/config.toml: RUSTFLAGS replaces it, so the
# flag is repeated here).
$env:RUSTFLAGS = "$($env:RUSTFLAGS) -C target-feature=+crt-static --remap-path-prefix=$PWD=. --remap-path-prefix=$env:USERPROFILE\.cargo=/cargo --remap-path-prefix=$env:USERPROFILE\.rustup=/rustup --remap-path-prefix=$env:USERPROFILE=/home"

Write-Host "==> Building (release)"
cargo build --release --locked
if ($LASTEXITCODE -ne 0) {
    Write-Error "The build failed. Copy the whole message to look into it."
}

$out = "dist\windows"
New-Item -ItemType Directory -Force -Path $out | Out-Null
if (Get-Process -Name "cuantoquedapp" -ErrorAction SilentlyContinue) {
    Write-Error "cuantoquedapp is running and locks the .exe. Quit it (right click on the icon > Quit) and run this script again."
}
Copy-Item "target\release\cuantoquedapp.exe" $out -Force
Copy-Item "target\release\cuantoquedapp-cli.exe" $out -Force

# The third-party license notices travel with the .exe when it is shared (see about.toml).
cargo about --version *> $null
if ($LASTEXITCODE -eq 0) {
    Write-Host "==> Writing the third-party licenses"
    cargo about generate --fail --locked about.hbs -o "$out\THIRD-PARTY-LICENSES.html"
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Could not write the third-party licenses (see the message above)."
    }
} else {
    Write-Warning "cargo-about is missing, so no THIRD-PARTY-LICENSES.html: fine for testing, required before sharing the .exe (cargo install cargo-about --locked --features cli)."
}

Write-Host ""
Write-Host "Done:"
Write-Host "  $out\cuantoquedapp.exe      (tray icon)"
Write-Host "  $out\cuantoquedapp-cli.exe  (limits in the terminal)"
