# Shared helpers for Kestrel build scripts. Dot-source this file.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$script:ClientDir = Join-Path $Root 'client'
$script:LauncherDir = Join-Path $Root 'launcher'
$script:DistDir = Join-Path $Root 'dist'

function Write-Step([string]$msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok([string]$msg) { Write-Host "    OK  $msg" -ForegroundColor Green }
function Write-Warn2([string]$msg) { Write-Host "    !!  $msg" -ForegroundColor Yellow }

# Run a native command and fail loudly on a non-zero exit code.
function Invoke-Native {
    param([Parameter(Mandatory)][string]$Exe, [string[]]$Arguments = @(), [string]$WorkDir = $Root)
    Push-Location $WorkDir
    try {
        & $Exe @Arguments
        if ($LASTEXITCODE -ne 0) { throw "'$Exe $($Arguments -join ' ')' failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

function Test-Command([string]$name) { return [bool](Get-Command $name -ErrorAction SilentlyContinue) }

# Major Java version of a java.exe, or 0 if it cannot be determined.
function Get-JavaMajor([string]$javaExe) {
    # `java -version` prints to stderr; in Windows PowerShell 5.1 redirected
    # native stderr becomes an error record, which 'Stop' would turn fatal.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $out = & $javaExe -version 2>&1 | Out-String
    } catch {
        return 0
    } finally {
        $ErrorActionPreference = $prev
    }
    if ($out -match 'version "1\.(\d+)') { return [int]$Matches[1] }
    if ($out -match 'version "(\d+)') { return [int]$Matches[1] }
    return 0
}

# Find a JDK (17-21) to run Gradle. Gradle downloads Java 8 for compiling the mod itself.
function Find-GradleJdk {
    $candidates = @()
    if ($env:JAVA_HOME) { $candidates += $env:JAVA_HOME }
    foreach ($base in @("$env:ProgramFiles\Eclipse Adoptium", "$env:ProgramFiles\Java", "$env:ProgramFiles\Microsoft", "$env:ProgramFiles\Zulu")) {
        if (Test-Path $base) {
            $candidates += Get-ChildItem $base -Directory | Where-Object { $_.Name -match '^(jdk|zulu)' } | Sort-Object Name -Descending | ForEach-Object { $_.FullName }
        }
    }
    # Note: $home is a read-only automatic variable in PowerShell.
    foreach ($jdkHome in $candidates) {
        $java = Join-Path $jdkHome 'bin\java.exe'
        if (Test-Path $java) {
            $major = Get-JavaMajor $java
            if ($major -ge 17 -and $major -le 21) { return $jdkHome }
        }
    }
    $onPath = Get-Command java -ErrorAction SilentlyContinue
    if ($onPath) {
        $major = Get-JavaMajor $onPath.Source
        if ($major -ge 17 -and $major -le 21) { return (Split-Path (Split-Path $onPath.Source)) }
    }
    return $null
}

function Find-VsWhere {
    $p = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $p) { return $p }
    return $null
}

function Test-MsvcTools {
    $vswhere = Find-VsWhere
    if (-not $vswhere) { return $false }
    $path = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$path
}

function Get-ModVersion {
    $props = Get-Content (Join-Path $ClientDir 'gradle.properties')
    foreach ($line in $props) { if ($line -match '^version=(.+)$') { return $Matches[1].Trim() } }
    throw 'version not found in client/gradle.properties'
}

function Get-LauncherVersion {
    $toml = Get-Content (Join-Path $LauncherDir 'Cargo.toml')
    $inWs = $false
    foreach ($line in $toml) {
        if ($line -match '^\[workspace\.package\]') { $inWs = $true; continue }
        if ($line -match '^\[') { $inWs = $false }
        if ($inWs -and $line -match '^version\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    throw 'version not found in launcher/Cargo.toml [workspace.package]'
}

# Update source configuration written by scripts\setup-updates.ps1.
# Returns $null when this checkout is not configured for updates.
function Get-UpdateConfig {
    $cfgPath = Join-Path $Root 'release.json'
    $keyPath = Join-Path $LauncherDir 'update-public-key.txt'
    if (-not (Test-Path $cfgPath) -or -not (Test-Path $keyPath)) { return $null }
    $cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json
    $key = (Get-Content $keyPath -Raw).Trim()
    if (-not $cfg.github_repo -or $cfg.github_repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw "release.json: github_repo must look like owner/repo" }
    if ($key -notmatch '^[A-Za-z0-9+/]{43}=$') { throw "launcher\update-public-key.txt does not contain a base64 Ed25519 public key" }
    [pscustomobject]@{
        Repo        = $cfg.github_repo
        ManifestUrl = "https://github.com/$($cfg.github_repo)/releases/latest/download/kestrel-update.json"
        PublicKey   = $key
        KeyId       = Get-KeyId $key
    }
}

# Same as kestrel_core::update::manifest::key_id: first 4 bytes of SHA-256(public key), hex.
function Get-KeyId([string]$publicKeyBase64) {
    $bytes = [Convert]::FromBase64String($publicKeyBase64.Trim())
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = $sha.ComputeHash($bytes) } finally { $sha.Dispose() }
    return -join ($hash[0..3] | ForEach-Object { $_.ToString('x2') })
}

# Runs git in the project root and returns trimmed stdout; throws on failure.
function Invoke-Git([string[]]$Arguments) {
    # Windows PowerShell 5.1 turns redirected native stderr into error records,
    # which 'Stop' would make fatal even when git succeeds (e.g. fetch progress).
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $out = & git -C $Root @Arguments 2>&1
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $prev
    }
    $text = (($out | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($code -ne 0) { throw "git $($Arguments -join ' ') failed: $text" }
    return $text
}

function Get-SigningKeyPath {
    if ($env:KESTREL_SIGNING_KEY) { return $env:KESTREL_SIGNING_KEY }
    return (Join-Path $env:APPDATA 'Kestrel-signing\update-signing-key.txt')
}
