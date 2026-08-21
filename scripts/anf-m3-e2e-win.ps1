<#
ANFAGENT-30 M3 Windows 控制面端到端冒烟（免安装客户端版）

流程：管理员登录 → 创建邀请码 → 设备注册（待审）→ 客户端以 --no-tun 用户态运行
      → 管理员放行 → 创建网络并分配 → 客户端收到托管配置并启动实例

安全设计：
  - 客户端使用 --no-tun（不创建虚拟网卡），避免与宿主机 etgame（2.4.5 mesh）冲突；
  - --no-listener，避免监听端口与既有 easytier 实例冲突；
  - 所有 HTTP 请求绑定 etgame 网卡源 IP（--interface），与交接文档 SSH 要求一致；
  - 仅新建测试设备/网络，不删除、不修改既有数据，不触碰 VM 官方 core。

用法：
  powershell -ExecutionPolicy Bypass -File scripts/anf-m3-e2e-win.ps1

依赖：
  - target/release/easytier-core.exe（本仓库 cargo build --release -p easytier 产物）
  - curl.exe（Windows 自带）
#>

param(
    [string]$WebBase = 'http://10.0.0.6:11211',
    [string]$ConfigServer = 'udp://10.0.0.6:22020/admin',
    [string]$BindIp = '10.0.0.3',
    [string]$CoreBin = 'D:\Project\anf-easytier\target\release\easytier-core.exe',
    [string]$NetworkName = 'anf-m3'
)

$ErrorActionPreference = 'Stop'
$Tmp = Join-Path $env:TEMP ('anf-m3-win-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp | Out-Null
$CookieFile = Join-Path $Tmp 'cookies.txt'
$ClientLog = Join-Path $Tmp 'client.log'

function Invoke-AnfApi {
    param(
        [string]$Method,
        [string]$Path,
        [string]$Body = '',
        [switch]$Auth
    )
    $url = "$WebBase$Path"
    $curlArgs = @('--interface', $BindIp, '-sS', '--noproxy', '*', '-X', $Method, $url, '-H', 'Content-Type: application/json')
    if ($Auth) { $curlArgs += @('-c', $CookieFile, '-b', $CookieFile) }
    if ($Body) {
        # 避免 PowerShell 5.1 原生参数传递剥掉 JSON 内嵌引号：body 走临时文件
        $bodyFile = Join-Path $Tmp ('body-' + [guid]::NewGuid().ToString('N') + '.json')
        [System.IO.File]::WriteAllText($bodyFile, $Body, (New-Object System.Text.UTF8Encoding($false)))
        $curlArgs += @('--data-binary', "@$bodyFile")
    }
    return (& curl.exe @curlArgs)
}

function Get-Md5Hex([string]$s) {
    $md5 = [System.Security.Cryptography.MD5]::Create()
    $bytes = $md5.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($s))
    return -join ($bytes | ForEach-Object { $_.ToString('x2') })
}

Write-Host "==> ANF M3 Windows 控制面 E2E（$WebBase，绑定 $BindIp，--no-tun）" -ForegroundColor Cyan
if (-not (Test-Path $CoreBin)) { throw "未找到 $CoreBin，请先构建：cargo build --release -p easytier" }

$DeviceMachine = [guid]::NewGuid().ToString()
$Ts = Get-Date -Format 'HHmmss'
$NetSuffix = Get-Random -Minimum 2 -Maximum 254

# 1. 管理员登录（默认账号 admin/admin，密码先 MD5）
Write-Host '== 1. 管理员登录'
$LoginResp = Invoke-AnfApi -Method POST -Path '/api/v1/auth/login' -Auth -Body ('{"username":"admin","password":"' + (Get-Md5Hex 'admin') + '"}')
Write-Host "    login: $LoginResp"
if (-not $LoginResp -or $LoginResp -match '"error"|401|Unauthorized') { throw "管理员登录失败：$LoginResp" }

# 2. 创建邀请码
Write-Host '== 2. 创建邀请码'
$InviteResp = Invoke-AnfApi -Method POST -Path '/api/v1/invites' -Auth -Body '{"max_uses":1}' | ConvertFrom-Json
$InviteCode = $InviteResp.code
Write-Host "    邀请码: $InviteCode"
if (-not $InviteCode) { throw "邀请码创建失败：$($InviteResp | ConvertTo-Json -Compress)" }

# 3. 设备注册（待审）
Write-Host '== 3. 设备注册（待审）'
$RegBody = @{ invite_code = $InviteCode; machine_id = $DeviceMachine } | ConvertTo-Json -Compress
$RegResp = Invoke-AnfApi -Method POST -Path '/api/v1/devices/register' -Body $RegBody | ConvertFrom-Json
$DeviceId = $RegResp.id
Write-Host "    设备 id: $DeviceId  状态: $($RegResp.status)"
if (-not $DeviceId) { throw "设备注册失败：$($RegResp | ConvertTo-Json -Compress)" }

# 4. 客户端以 --no-tun 用户态运行（待审阶段应不入网）
Write-Host '== 4. 启动客户端（--no-tun --no-listener，待审）'
$clientArgs = @('-w', $ConfigServer, '--machine-id', $DeviceMachine, '--network-name', $NetworkName, '--no-tun', '--no-listener')
$proc = Start-Process -FilePath $CoreBin -ArgumentList $clientArgs -RedirectStandardOutput $ClientLog -RedirectStandardError "$ClientLog.err" -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 8
Write-Host '    -- 客户端日志（待审阶段）--'
Get-Content $ClientLog, "$ClientLog.err" -ErrorAction SilentlyContinue | Select-Object -Last 8

# 5. 管理员放行
Write-Host "== 5. 放行设备 $DeviceId"
Invoke-AnfApi -Method POST -Path "/api/v1/devices/$DeviceId/approve" -Auth | Out-Null
Write-Host '    已放行'

# 6. 创建网络（唯一名称，避免与既有数据冲突）
Write-Host '== 6. 创建网络并分配'
$NetResp = Invoke-AnfApi -Method POST -Path '/api/v1/networks' -Auth -Body ('{"name":"win-test-' + $Ts + '-' + $NetSuffix + '","cidr":"10.99.' + $NetSuffix + '.0/24"}') | ConvertFrom-Json
$NetId = $NetResp.id
Write-Host "    网络 id: $NetId"

# 7. 分配设备到网络（触发托管配置生成/下发）
$PatchBody = @{ tags = @('办公'); networks = @($NetId) } | ConvertTo-Json -Compress
Invoke-AnfApi -Method PATCH -Path "/api/v1/devices/$DeviceId" -Auth -Body $PatchBody | Out-Null
Write-Host "    设备 $DeviceId 已分配网络 $NetId（tag: 办公）"

# 8. 等待配置下发与实例启动
Write-Host '== 8. 等待配置下发与实例启动（12s）'
Start-Sleep -Seconds 12
Write-Host '    -- 客户端日志（放行后）--'
Get-Content $ClientLog, "$ClientLog.err" -ErrorAction SilentlyContinue | Select-Object -Last 15

# 9. 核对设备状态
$DevState = Invoke-AnfApi -Method GET -Path "/api/v1/devices/$DeviceId" -Auth | ConvertFrom-Json
Write-Host "    设备状态: $($DevState.status)"

# 10. 清理
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Host "== 完成（日志目录: $Tmp）"
