# ANF M3 connect smoke test (no etgame NIC binding; direct to VM control plane)
# Flow: admin login -> create invite -> register device(pending) -> core client --no-tun connects config-server
#       -> admin approve -> create network + assign -> client receives managed config and runs instance
# Depends on: target/release/anf-easytier-core.exe (cargo build --release -p easytier)
# NOTE: the config-server pushes a managed config that creates a WinTun adapter. To reach a RUNNING instance,
#       run under Administrator and install Npcap; otherwise the instance logs a WinTun access-denied error.
param(
    [string]$WebBase = 'http://10.0.0.6:11211',
    [string]$ConfigServer = 'udp://10.0.0.6:22020/admin',
    [string]$CoreBin = 'D:\Project\anf-easytier\target\release\anf-easytier-core.exe',
    [string]$NetworkName = 'anf-m3'
)
$ErrorActionPreference = 'Stop'
$Tmp = Join-Path $env:TEMP ('anf-m3-conn-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp | Out-Null
$CookieFile = Join-Path $Tmp 'cookies.txt'
$ClientLog = Join-Path $Tmp 'client.log'
$ClientErr = "$ClientLog.err"

function Invoke-AnfApi {
    param([string]$Method,[string]$Path,[string]$Body='',[switch]$Auth)
    $url = "$WebBase$Path"
    $args = @('-sS','--noproxy','*','--max-time','15','-X',$Method,$url,'-H','Content-Type: application/json')
    if ($Auth) { $args += @('-c',$CookieFile,'-b',$CookieFile) }
    if ($Body) {
        $bf = Join-Path $Tmp ('body-' + [guid]::NewGuid().ToString('N') + '.json')
        [System.IO.File]::WriteAllText($bf,$Body,(New-Object System.Text.UTF8Encoding($false)))
        $args += @('--data-binary',"@$bf")
    }
    return (& curl.exe @args)
}
function Get-Md5Hex([string]$s) {
    $m=[System.Security.Cryptography.MD5]::Create(); $b=$m.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($s)); -join ($b | ForEach-Object { $_.ToString('x2') })
}

Write-Host "==> ANF M3 connect smoke ($WebBase)" -ForegroundColor Cyan
if (-not (Test-Path $CoreBin)) { throw "core missing: $CoreBin" }

$DeviceMachine = [guid]::NewGuid().ToString()
$Ts = Get-Date -Format 'HHmmss'
$NetSuffix = Get-Random -Minimum 2 -Maximum 254

Write-Host '== 1. admin login'
$LoginResp = Invoke-AnfApi -Method POST -Path '/api/v1/auth/login' -Auth -Body ('{"username":"admin","password":"' + (Get-Md5Hex 'admin') + '"}')
Write-Host "    login: $LoginResp"

Write-Host '== 2. create invite'
$InviteResp = Invoke-AnfApi -Method POST -Path '/api/v1/invites' -Auth -Body '{"max_uses":1}' | ConvertFrom-Json
$InviteCode = $InviteResp.code
Write-Host "    invite: $InviteCode"

Write-Host '== 3. register device (pending)'
$RegBody = @{ invite_code = $InviteCode; machine_id = $DeviceMachine } | ConvertTo-Json -Compress
$RegResp = Invoke-AnfApi -Method POST -Path '/api/v1/devices/register' -Body $RegBody | ConvertFrom-Json
$DeviceId = $RegResp.id
Write-Host "    device id: $DeviceId  status: $($RegResp.status)"

Write-Host '== 4. start client (--no-tun, pending)'
$clientArgs = @('-w', $ConfigServer, '--machine-id', $DeviceMachine, '--network-name', $NetworkName, '--no-tun=true', '--use-smoltcp', '--no-listener')
$proc = Start-Process -FilePath $CoreBin -ArgumentList $clientArgs -RedirectStandardOutput $ClientLog -RedirectStandardError $ClientErr -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 8
Write-Host '    -- pending client log --'
Get-Content $ClientLog, $ClientErr -ErrorAction SilentlyContinue | Select-Object -Last 8

Write-Host '== 5. approve device'
Invoke-AnfApi -Method POST -Path "/api/v1/devices/$DeviceId/approve" -Auth | Out-Null
Write-Host '    approved'

Write-Host '== 6. create network + assign'
$NetResp = Invoke-AnfApi -Method POST -Path '/api/v1/networks' -Auth -Body ('{"name":"conn-' + $Ts + '-' + $NetSuffix + '","cidr":"10.99.' + $NetSuffix + '.0/24"}') | ConvertFrom-Json
$NetId = $NetResp.id
Write-Host "    network id: $NetId"
$PatchBody = @{ tags = @('office'); networks = @($NetId) } | ConvertTo-Json -Compress
Invoke-AnfApi -Method PATCH -Path "/api/v1/devices/$DeviceId" -Auth -Body $PatchBody | Out-Null
Write-Host "    assigned network $NetId"

Write-Host '== 7. wait for config push + instance start (15s)'
Start-Sleep -Seconds 15
Write-Host '    -- client log (after approve) --'
Get-Content $ClientLog, $ClientErr -ErrorAction SilentlyContinue | Select-Object -Last 20

Write-Host '== 8. device status'
$DevState = Invoke-AnfApi -Method GET -Path "/api/v1/devices/$DeviceId" -Auth | ConvertFrom-Json
Write-Host "    device status: $($DevState.status)  networks: $($DevState.networks -join ',')"

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Host "== done (log dir: $Tmp)"
