# Shared loader: load repo-root .env into the current process environment.
# Existing environment variables always win. Dot-source from scripts/:
#   . (Join-Path $PSScriptRoot '_anf_env.ps1')

function Import-AnfEnv {
    param([string]$Path = '')
    $envFile = if ($Path) { $Path } else { Join-Path (Split-Path $PSScriptRoot -Parent) '.env' }
    if (-not (Test-Path $envFile)) { return }
    Get-Content $envFile | ForEach-Object {
        $line = $_.Trim()
        if ($line -and -not $line.StartsWith('#') -and $line.Contains('=')) {
            $k, $v = $line.Split('=', 2)
            $k = $k.Trim()
            $v = $v.Trim()
            if ($k -and -not [Environment]::GetEnvironmentVariable($k)) {
                Set-Item -Path "Env:$k" -Value $v
            }
        }
    }
}

Import-AnfEnv
