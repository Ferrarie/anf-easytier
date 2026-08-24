<#
用法:
  scripts/bump-version.ps1 -Version 1.0.1   # 指定完整版本
  scripts/bump-version.ps1 -Major           # 主版本 +1，次/补丁清零
  scripts/bump-version.ps1 -Minor           # 次版本 +1，补丁清零
  scripts/bump-version.ps1 -Patch           # 补丁版本 +1

以 easytier-gui/package.json 为唯一真源，同步 Cargo.toml 与 tauri.conf.json。
#>
param(
  [string]$Version,
  [switch]$Major,
  [switch]$Minor,
  [switch]$Patch
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$GuiDir = Join-Path $RepoRoot 'easytier-gui'
$PkgJson = Join-Path $GuiDir 'package.json'
$CargoToml = Join-Path $GuiDir 'src-tauri\Cargo.toml'
$TauriConf = Join-Path $GuiDir 'src-tauri\tauri.conf.json'

$pkg = Get-Content -LiteralPath $PkgJson -Raw | ConvertFrom-Json
$current = [string]$pkg.version
if (-not $current) { throw '无法从 package.json 读取 version' }

if ($Version) {
  $newVersion = $Version
}
elseif ($Major -or $Minor -or $Patch) {
  $parts = $current -split '\.'
  if ($parts.Count -lt 3) { throw "无法解析当前版本: $current" }
  $maj = [int]$parts[0]; $min = [int]$parts[1]; $pat = [int]$parts[2]
  if ($Major) { $maj++; $min = 0; $pat = 0 }
  elseif ($Minor) { $min++; $pat = 0 }
  else { $pat++ }
  $newVersion = "$maj.$min.$pat"
}
else {
  throw '请提供 -Version <x.y.z> 或 -Major/-Minor/-Patch 之一'
}

if ($newVersion -notmatch '^\d+\.\d+\.\d+$') {
  throw "版本号必须是 x.y.z 格式，当前输入: $newVersion"
}

# 1) package.json（真源）
$pkgText = Get-Content -LiteralPath $PkgJson -Raw
$pkgText = [regex]::Replace($pkgText, '"version"\s*:\s*"[^"]+"', "`"version`": `"$newVersion`"", 1)
Set-Content -LiteralPath $PkgJson -Value $pkgText -Encoding UTF8 -NoNewline

# 2) Cargo.toml（[package] 下的 version = "x.y.z"）
$cargoText = Get-Content -LiteralPath $CargoToml -Raw
$cargoText = [regex]::Replace($cargoText, '(?m)^version\s*=\s*"[^"]+"', "version = `"$newVersion`"", 1)
Set-Content -LiteralPath $CargoToml -Value $cargoText -Encoding UTF8 -NoNewline

# 3) tauri.conf.json
$tauriText = Get-Content -LiteralPath $TauriConf -Raw
$tauriText = [regex]::Replace($tauriText, '"version"\s*:\s*"[^"]+"', "`"version`": `"$newVersion`"", 1)
Set-Content -LiteralPath $TauriConf -Value $tauriText -Encoding UTF8 -NoNewline

Write-Host "版本已同步: $current -> $newVersion"
Write-Host "  package.json    : $((Get-Content $PkgJson -Raw | ConvertFrom-Json).version)"
Write-Host "  Cargo.toml      : $((Select-String -Path $CargoToml -Pattern '^version\s*=' | Select-Object -First 1).Line)"
Write-Host "  tauri.conf.json : $(((Get-Content $TauriConf -Raw | ConvertFrom-Json).version))"
