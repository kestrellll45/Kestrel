<#
.SYNOPSIS
  Sets the Kestrel version (launcher + client mod) in one step.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\bump-version.ps1 -Version 0.1.1
#>
param([Parameter(Mandatory)][string]$Version)
. (Join-Path $PSScriptRoot 'common.ps1')

if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { throw "'$Version' is not a semantic version like 0.1.1" }
$old = Get-LauncherVersion

$tomlPath = Join-Path $LauncherDir 'Cargo.toml'
$toml = Get-Content $tomlPath
$inWs = $false
$toml = foreach ($line in $toml) {
    if ($line -match '^\[workspace\.package\]') { $inWs = $true }
    elseif ($line -match '^\[') { $inWs = $false }
    if ($inWs -and $line -match '^version\s*=') { "version = `"$Version`"" } else { $line }
}
# UTF-8 without a byte-order mark (Windows PowerShell's -Encoding utf8 adds one).
[System.IO.File]::WriteAllLines($tomlPath, [string[]]$toml, (New-Object System.Text.UTF8Encoding($false)))

$propsPath = Join-Path $ClientDir 'gradle.properties'
$props = (Get-Content $propsPath) | ForEach-Object { if ($_ -match '^version=') { "version=$Version" } else { $_ } }
Set-Content -Path $propsPath -Value $props -Encoding ascii

if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) { $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path" }
# Refresh Cargo.lock for the workspace crates only (release builds use --locked).
Invoke-Native 'cargo' @('update', '--workspace', '--offline') $LauncherDir

Write-Ok "version $old -> $Version (launcher/Cargo.toml, client/gradle.properties, Cargo.lock)"
