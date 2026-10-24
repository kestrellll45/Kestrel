<#
.SYNOPSIS
  One-time setup for automatic launcher updates.
.DESCRIPTION
  1. Builds the publisher tool (kestrel-release).
  2. Creates your Ed25519 release-signing key in %APPDATA%\Kestrel-signing\
     (outside the project; only your Windows account can read it), or reuses
     the existing one.
  3. Writes the PUBLIC key to launcher\update-public-key.txt and the GitHub
     repository to release.json. Both are safe to commit.
  Every Kestrel.exe built afterwards with scripts\release.ps1 checks
  https://github.com/<repo>/releases/latest for updates and only accepts
  releases signed with this key.

  BACK UP the private key file. If it is lost, existing launchers cannot
  accept new updates and every user would need one manual reinstall.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\setup-updates.ps1 -Repo yourname/kestrel
#>
param([Parameter(Mandatory)][string]$Repo)
. (Join-Path $PSScriptRoot 'common.ps1')

if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw "-Repo must look like owner/repo (got '$Repo')" }
if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) { $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path" }

Write-Step 'Building the release tool'
Invoke-Native 'cargo' @('build', '--release', '--locked', '-p', 'kestrel-release') $LauncherDir
$tool = Join-Path $LauncherDir 'target\release\kestrel-release.exe'

$keyPath = Get-SigningKeyPath
$keyDir = Split-Path $keyPath
if (Test-Path $keyPath) {
    Write-Ok "using existing signing key $keyPath"
    $public = (& $tool pubkey $keyPath).Trim()
    if ($LASTEXITCODE -ne 0) { throw "cannot read $keyPath" }
} else {
    Write-Step "Creating signing key $keyPath"
    New-Item -ItemType Directory -Force -Path $keyDir | Out-Null
    # Only the current user may access the key folder.
    & icacls $keyDir /inheritance:r /grant:r "$($env:USERNAME):(OI)(CI)F" | Out-Null
    $public = (& $tool keygen $keyPath).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'key generation failed' }
    Write-Ok 'signing key created'
}

$pubFile = Join-Path $LauncherDir 'update-public-key.txt'
Set-Content -Path $pubFile -Value $public -Encoding ascii
@{ github_repo = $Repo } | ConvertTo-Json | Set-Content -Path (Join-Path $Root 'release.json') -Encoding ascii
Write-Ok "public key  -> $pubFile"
Write-Ok "repository  -> release.json ($Repo)"

Write-Host ''
Write-Host 'Next steps:' -ForegroundColor Green
Write-Host "  1. Back up $keyPath somewhere safe (password manager / USB stick). Never commit or share it."
Write-Host "  2. Make sure https://github.com/$Repo exists and is PUBLIC (launchers download releases anonymously)."
Write-Host '  3. Build an update-enabled launcher:  powershell -ExecutionPolicy Bypass -File scripts\release.ps1'
Write-Host '  4. Replace your Kestrel.exe with release\Kestrel.exe once. Future versions arrive through the launcher.'
