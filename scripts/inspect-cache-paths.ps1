$Root = Split-Path -Parent $PSScriptRoot
$paths = [ordered]@{
  NPM_CONFIG_CACHE = $env:NPM_CONFIG_CACHE
  PNPM_STORE_DIR = $env:PNPM_STORE_DIR
  CARGO_HOME = $env:CARGO_HOME
  RUSTUP_HOME = if ([string]::IsNullOrWhiteSpace($env:RUSTUP_HOME)) { "<default rustup home>" } else { $env:RUSTUP_HOME }
  TEMP = $env:TEMP
  TMP = $env:TMP
}

Write-Host "Repository: $Root"
foreach ($item in $paths.GetEnumerator()) {
  $value = if ([string]::IsNullOrWhiteSpace($item.Value)) { "<not set>" } else { $item.Value }
  Write-Host "$($item.Key): $value"
}
