<#
以管理员权限运行 anf-easytier 客户端（easytier-core），用于创建 TUN 虚拟网卡的验证。
用法（普通 shell 触发，会弹 UAC）：
  Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','D:\Project\anf-easytier\scripts\run-anf-client-elevated.ps1','-ConfigServer','udp://10.0.0.6:22020/admin','-MachineId','<uuid>'
停止（同样提权）：
  ... -File scripts\run-anf-client-elevated.ps1 -Kill
#>

param(
    [string]$ConfigServer,
    [string]$MachineId,
    [string]$NetworkName = 'anf-m3',
    [string]$CoreBin = 'D:\Project\anf-easytier\target\release\easytier-core.exe',
    [string]$LogDir = "$env:TEMP\anf-client",
    [switch]$Kill
)

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null

if ($Kill) {
    Get-Process -Name 'easytier-core' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $CoreBin } |
        Stop-Process -Force
    Write-Output 'client stopped'
    exit 0
}

$stdout = Join-Path $LogDir 'client.out.log'
$stderr = Join-Path $LogDir 'client.err.log'
$cliArgs = @('-w', $ConfigServer, '--machine-id', $MachineId, '--network-name', $NetworkName, '--no-listener', '--bind-device', 'false')
$p = Start-Process -FilePath $CoreBin -ArgumentList $cliArgs `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -WindowStyle Hidden
Set-Content -Path (Join-Path $LogDir 'client.pid') -Value $p.Id
Write-Output "client started pid=$($p.Id)"
