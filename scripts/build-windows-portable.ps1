<#
ANF EasyTier Windows 免安装客户端打包脚本

用法（PowerShell）：
    powershell -ExecutionPolicy Bypass -File scripts/build-windows-portable.ps1

产物：
    dist/anf-easytier-win-x64-<版本>.zip
    （内含 anf-easytier.exe + wintun.dll + Packet.dll + WinDivert64.sys + 使用说明）

前置依赖：
    - Visual Studio（脚本自动定位 vcvars64.bat）
    - LLVM（LIBCLANG_PATH，默认 C:\Program Files\LLVM\bin）
    - protoc（$env:PROTOC 或 %TEMP%\protoc-win\bin\protoc.exe 或 PATH）
    - 7-Zip（thunk-rs 构建需要，默认 C:\Program Files\7-Zip）
    - pnpm / node / cargo

运行依赖（发给测试者时提示）：
    - Windows 10/11（WebView2 运行时自带）
    - 建 TUN 需要 Npcap：winget install Npcap.Npcap（装驱动，可能需重启）
#>

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$GuiDir = Join-Path $RepoRoot 'easytier-gui'
$ThirdPartyDir = Join-Path $RepoRoot 'easytier\third_party\x86_64'

# 从 tauri.conf.json 读取版本，避免两处维护
$TauriConf = Get-Content (Join-Path $GuiDir 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$Version = $TauriConf.version
$ProductName = $TauriConf.productName
if (-not $ProductName -or -not $Version) {
    throw "无法从 tauri.conf.json 读取 productName/version"
}

$ArtifactName = "anf-easytier-win-x64-$Version"
$DistDir = Join-Path $RepoRoot 'dist'
$StageDir = Join-Path $DistDir $ArtifactName

Write-Host "==> ANF EasyTier Windows 免安装打包（$Version）" -ForegroundColor Cyan

# ---------- 1. 前置检查 ----------
function Find-Command([string]$Name) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    return $cmd -ne $null
}

if (-not (Find-Command 'pnpm')) { throw '未找到 pnpm，请先安装 Node.js/pnpm' }
if (-not (Find-Command 'cargo')) { throw '未找到 cargo，请先安装 Rust 工具链' }

# VS vcvars64.bat
$vcvarsCandidates = @()
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if ($vsPath) { $vcvarsCandidates += (Join-Path $vsPath 'VC\Auxiliary\Build\vcvars64.bat') }
}
$vcvarsCandidates += 'C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat'
$vcvarsCandidates += 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat'
$VcVars = $vcvarsCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $VcVars) { throw '未找到 vcvars64.bat（VS C++ 工作负载），请安装 Visual Studio C++ 工具链' }

# LLVM / libclang
if (-not $env:LIBCLANG_PATH) {
    if (Test-Path 'C:\Program Files\LLVM\bin') { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
    elseif (Test-Path 'C:\Program Files (x86)\LLVM\bin') { $env:LIBCLANG_PATH = 'C:\Program Files (x86)\LLVM\bin' }
}
if (-not $env:LIBCLANG_PATH -or -not (Test-Path (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) {
    throw '未找到 libclang.dll，请安装 LLVM 并设置 LIBCLANG_PATH（如 C:\Program Files\LLVM\bin）'
}

# protoc
if (-not $env:PROTOC) {
    $protocTemp = Join-Path $env:TEMP 'protoc-win\bin\protoc.exe'
    if (Test-Path $protocTemp) { $env:PROTOC = $protocTemp }
    elseif (Find-Command 'protoc') { $env:PROTOC = (Get-Command 'protoc').Source }
}
if (-not $env:PROTOC -or -not (Test-Path $env:PROTOC)) {
    throw '未找到 protoc，请下载 protoc 到 %TEMP%\protoc-win\bin 或设置 $env:PROTOC'
}

# corepack 缓存（默认 %LOCALAPPDATA%\node\corepack 的 ACL 可能损坏，重定向到 ~\.corepack）
if (-not $env:COREPACK_HOME) {
    $env:COREPACK_HOME = Join-Path $HOME '.corepack'
}
New-Item -ItemType Directory -Path $env:COREPACK_HOME -Force | Out-Null

# 7-Zip（thunk-rs 构建时需要）
if (-not (Find-Command '7z')) {
    $sevenZip = 'C:\Program Files\7-Zip\7z.exe'
    if (Test-Path $sevenZip) { $env:PATH = "C:\Program Files\7-Zip;$env:PATH" }
    else { throw '未找到 7-Zip（thunk-rs 构建依赖），请安装或加入 PATH' }
}

Write-Host "    vcvars64.bat : $VcVars"
Write-Host "    LIBCLANG_PATH : $env:LIBCLANG_PATH"
Write-Host "    PROTOC        : $env:PROTOC"

# ---------- 2. 前置 JS 包构建（tauri-plugin-vpnservice-api 需要 dist-js） ----------
$PluginDir = Join-Path $RepoRoot 'tauri-plugin-vpnservice'
if (-not (Test-Path (Join-Path $PluginDir 'dist-js\index.d.ts'))) {
    Write-Host '==> 构建 tauri-plugin-vpnservice-api（生成 dist-js）' -ForegroundColor Cyan
    pnpm --dir $PluginDir build
    if ($LASTEXITCODE -ne 0) { throw "tauri-plugin-vpnservice build 失败（exit code $LASTEXITCODE）" }
}

# ---------- 3. 确保 Windows 驱动资源就位（tauri-build 校验 resources） ----------
$WindowsResources = @('wintun.dll', 'Packet.dll', 'WinDivert64.sys')
foreach ($f in $WindowsResources) {
    $dst = Join-Path $GuiDir "src-tauri\$f"
    if (-not (Test-Path $dst)) {
        $src = Join-Path $ThirdPartyDir $f
        if (-not (Test-Path $src)) { throw "缺少驱动资源 $f（third_party/x86_64 下找不到）" }
        Copy-Item -LiteralPath $src -Destination $dst
        Write-Host "    已补全资源：$f"
    }
}

# ---------- 4. 构建（vcvars + 环境变量 + pnpm tauri build --no-bundle） ----------
Write-Host '==> 开始构建（pnpm tauri build --no-bundle，可能耗时数分钟）' -ForegroundColor Cyan

$buildCmd = "call `"$VcVars`" && set `"LIBCLANG_PATH=$env:LIBCLANG_PATH`" && set `"PROTOC=$env:PROTOC`" && set `"PATH=C:\Program Files\7-Zip;%PATH%`" && pnpm --dir `"$GuiDir`" tauri build --no-bundle"
cmd /c $buildCmd
if ($LASTEXITCODE -ne 0) { throw "tauri build 失败（exit code $LASTEXITCODE）" }

# ---------- 5. 收集产物 ----------
$ExeCandidates = @(
    (Join-Path $RepoRoot "target\release\$ProductName.exe"),
    (Join-Path $GuiDir "src-tauri\target\release\$ProductName.exe")
)
$Exe = $ExeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $Exe) { throw "未找到构建产物 $ProductName.exe" }

$DllSources = @('wintun.dll', 'Packet.dll', 'WinDivert64.sys') | ForEach-Object {
    $p = Join-Path $ThirdPartyDir $_
    if (Test-Path $p) { return $p }
    $p2 = Join-Path $GuiDir "src-tauri\$_"
    if (Test-Path $p2) { return $p2 }
    throw "缺少驱动资源文件 $_（third_party/x86_64 或 src-tauri 下）"
}

# ---------- 6. 组装发布目录 ----------
if (Test-Path $StageDir) { Remove-Item -LiteralPath $StageDir -Recurse -Force }
New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

Copy-Item -LiteralPath $Exe -Destination $StageDir
foreach ($dll in $DllSources) { Copy-Item -LiteralPath $dll -Destination $StageDir }

$readme = @"
ANF EasyTier 客户端（免安装版） v$Version
==========================================

1. 解压后双击 anf-easytier.exe 即可运行（无需安装）。
2. 首次建 TUN 网卡前请安装 Npcap（管理员运行）：
      winget install Npcap.Npcap
   装驱动后如无网卡出现，请重启电脑。
3. 本版本为 ANF 中心化组网客户端，不兼容 easytier 2.4.5 旧客户端/旧节点，
   入网需中心平台（easytier-web）审批放行。
4. 文件清单：
      anf-easytier.exe  客户端主程序
      wintun.dll        TUN 驱动（虚拟网卡）
      Packet.dll        数据包捕获（Npcap 兼容层）
      WinDivert64.sys   UDP 广播捕获驱动

构建时间：$(Get-Date -Format 'yyyy-MM-dd HH:mm')
"@
Set-Content -LiteralPath (Join-Path $StageDir 'README.txt') -Value $readme -Encoding UTF8

# ---------- 7. 压缩 ----------
$ZipPath = Join-Path $DistDir "$ArtifactName.zip"
if (Test-Path $ZipPath) { Remove-Item -LiteralPath $ZipPath -Force }
Compress-Archive -Path $StageDir -DestinationPath $ZipPath -CompressionLevel Optimal

# ---------- 8. 摘要 ----------
Write-Host ''
Write-Host '==> 打包完成' -ForegroundColor Green
Write-Host "    发布目录 : $StageDir"
Write-Host "    zip      : $ZipPath"
Get-ChildItem $StageDir | Select-Object Name, Length | Format-Table -AutoSize

# Npcap 提醒
if (-not (Get-Service | Where-Object { $_.Name -like '*npcap*' })) {
    Write-Host '    提醒：本机未检测到 Npcap，建 TUN 前需安装（winget install Npcap.Npcap）' -ForegroundColor Yellow
}
