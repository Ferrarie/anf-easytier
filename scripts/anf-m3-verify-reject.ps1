# Runtime check (new semantics): default list shows ALL devices (incl. rejected/kicked);
# status=rejected filter still works; same machine_id re-register revives to pending; delete removes.
param(
    [string]$WebBase = 'http://10.0.0.6:11211'
)
$ErrorActionPreference = 'Stop'
$Tmp = Join-Path $env:TEMP ('anf-m3-verify-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp | Out-Null
$CookieFile = Join-Path $Tmp 'cookies.txt'

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

Invoke-AnfApi -Method POST -Path '/api/v1/auth/login' -Auth -Body ('{"username":"admin","password":"' + (Get-Md5Hex 'admin') + '"}') | Out-Null
$Invite = (Invoke-AnfApi -Method POST -Path '/api/v1/invites' -Auth -Body '{"max_uses":2}' | ConvertFrom-Json).code
$m = [guid]::NewGuid().ToString()
$reg = Invoke-AnfApi -Method POST -Path '/api/v1/devices/register' -Body (@{ invite_code=$Invite; machine_id=$m } | ConvertTo-Json -Compress) | ConvertFrom-Json
$id = $reg.id
Write-Host "registered id=$id machine=$($m.Substring(0,8)) status=$($reg.status)"

Invoke-AnfApi -Method POST -Path "/api/v1/devices/$id/reject" -Auth | Out-Null
Write-Host 'rejected.'

$default = Invoke-AnfApi -Method GET -Path '/api/v1/devices' -Auth | ConvertFrom-Json
$shownInDefault = [bool]($default | Where-Object { $_.id -eq $id })
Write-Host ("default list shows rejected (ALL): {0}" -f $shownInDefault)

$rej = Invoke-AnfApi -Method GET -Path '/api/v1/devices?status=rejected' -Auth | ConvertFrom-Json
$visibleInFilter = [bool]($rej | Where-Object { $_.id -eq $id })
Write-Host ("visible when status=rejected: {0}" -f $visibleInFilter)

$reg2 = Invoke-AnfApi -Method POST -Path '/api/v1/devices/register' -Body (@{ invite_code=$Invite; machine_id=$m } | ConvertTo-Json -Compress) | ConvertFrom-Json
Write-Host ("re-register same machine -> id=$($reg2.id) status=$($reg2.status)")

$default2 = Invoke-AnfApi -Method GET -Path '/api/v1/devices' -Auth | ConvertFrom-Json
$revived = [bool]($default2 | Where-Object { $_.id -eq $id })
Write-Host ("revived in default list after re-register: {0}" -f $revived)

$del = Invoke-AnfApi -Method DELETE -Path "/api/v1/devices/$id" -Auth
Write-Host ("delete device: {0}" -f $del)
$default3 = Invoke-AnfApi -Method GET -Path '/api/v1/devices' -Auth | ConvertFrom-Json
$gone = -not ($default3 | Where-Object { $_.id -eq $id })
Write-Host ("removed after delete: {0}" -f $gone)

if (-not $shownInDefault) { Write-Error 'FAIL: rejected device not shown in default list' }
if (-not $visibleInFilter) { Write-Error 'FAIL: rejected device not visible via status=rejected' }
if (-not $revived) { Write-Error 'FAIL: device not revived after re-register' }
if (-not $gone) { Write-Error 'FAIL: device not removed after delete' }
Write-Host '== done'
