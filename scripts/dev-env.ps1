$Root = Split-Path -Parent $PSScriptRoot
$env:NPM_CONFIG_CACHE = Join-Path $Root ".cache\npm"
$env:PNPM_STORE_DIR = Join-Path $Root ".pnpm-store"
$env:CARGO_HOME = Join-Path $Root ".cargo"
$env:TAURI_CLI_NO_DEV_SERVER_WAIT = ""
$TempRoot = Join-Path $Root ".cache\tmp"
New-Item -ItemType Directory -Force -Path $env:NPM_CONFIG_CACHE, $env:PNPM_STORE_DIR, $env:CARGO_HOME, $TempRoot | Out-Null
$env:TEMP = $TempRoot
$env:TMP = $TempRoot
$UserCargoShimBin = Join-Path $HOME ".cargo\bin"

# Use the system/default Rust toolchain. Do not force RUSTUP_HOME to the repo,
# because rustup shims would otherwise try to install a second local toolchain.
Remove-Item Env:RUSTUP_HOME -ErrorAction SilentlyContinue

if ((-not (Get-Command rustc -ErrorAction SilentlyContinue) -or -not (Get-Command cargo -ErrorAction SilentlyContinue)) -and (Test-Path $UserCargoShimBin) -and (($env:Path -split ';') -notcontains $UserCargoShimBin)) {
  $env:Path = "$UserCargoShimBin;$env:Path"
}
Write-Host "Tool caches pinned under $Root"
