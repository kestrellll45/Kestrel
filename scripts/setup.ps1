<#
.SYNOPSIS
  Checks (and optionally installs) everything needed to build Kestrel on Windows.
.DESCRIPTION
  Required:
    - Rust (rustup, stable, MSVC toolchain)       -> launcher
    - Visual Studio Build Tools with C++ workload  -> Rust linker + TLS crypto
    - A JDK 17-21                                  -> runs Gradle (it downloads Java 8 for the mod itself)
  Missing tools are installed with winget when you confirm (or always with -Yes).
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\setup.ps1 -Yes
#>
param([switch]$Yes)
. (Join-Path $PSScriptRoot 'common.ps1')

if ($env:OS -ne 'Windows_NT') { throw 'Kestrel targets Windows; run this on Windows.' }

function Confirm-Install([string]$what) {
    if ($Yes) { return $true }
    $a = Read-Host "Install $what now with winget? [Y/n]"
    return ($a -eq '' -or $a -match '^[Yy]')
}

function Install-Winget([string]$id, [string[]]$extra = @()) {
    if (-not (Test-Command 'winget')) { throw 'winget is not available. Install "App Installer" from the Microsoft Store, or install the tool manually.' }
    $wgArgs = @('install', '--id', $id, '-e', '--accept-source-agreements', '--accept-package-agreements') + $extra
    Invoke-Native 'winget' $wgArgs
}

$missing = 0

Write-Step 'Windows'
Write-Ok ([System.Environment]::OSVersion.VersionString)
$freeGb = [math]::Round((Get-PSDrive -Name ($Root.Substring(0, 1))).Free / 1GB, 1)
if ($freeGb -lt 6) { Write-Warn2 "Only $freeGb GB free; a full build plus game files needs about 6 GB." } else { Write-Ok "$freeGb GB free" }

Write-Step 'Visual Studio C++ Build Tools (needed by Rust on Windows)'
if (Test-MsvcTools) {
    Write-Ok 'MSVC tools found'
} elseif (Confirm-Install 'Visual Studio 2022 Build Tools (C++ workload, ~3 GB)') {
    Install-Winget 'Microsoft.VisualStudio.2022.BuildTools' @('--override', '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
    if (Test-MsvcTools) { Write-Ok 'installed' } else { Write-Warn2 'installed; restart your terminal and re-run setup'; $missing++ }
} else { Write-Warn2 'skipped'; $missing++ }

Write-Step 'Rust'
if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}
if (Test-Command 'cargo') {
    Write-Ok (& cargo --version)
} elseif (Confirm-Install 'Rust (rustup)') {
    Install-Winget 'Rustlang.Rustup'
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    Invoke-Native 'rustup' @('default', 'stable-x86_64-pc-windows-msvc')
    Write-Ok (& cargo --version)
} else { Write-Warn2 'skipped'; $missing++ }
if (Test-Command 'rustup') {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & rustup component add rustfmt clippy *> $null
    $ErrorActionPreference = $prev
}

Write-Step 'JDK 17-21 for Gradle'
$jdk = Find-GradleJdk
if ($jdk) {
    Write-Ok "using $jdk"
} elseif (Confirm-Install 'Eclipse Temurin JDK 17') {
    Install-Winget 'EclipseAdoptium.Temurin.17.JDK'
    $jdk = Find-GradleJdk
    if ($jdk) { Write-Ok "installed at $jdk" } else { Write-Warn2 'installed; restart your terminal and re-run setup'; $missing++ }
} else { Write-Warn2 'skipped'; $missing++ }

Write-Step 'Network access to build dependencies'
foreach ($h in @('https://maven.minecraftforge.net/', 'https://repo.spongepowered.org/maven/', 'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json', 'https://index.crates.io/config.json', 'https://services.gradle.org/')) {
    try {
        $null = Invoke-WebRequest -Uri $h -Method Head -UseBasicParsing -TimeoutSec 15
        Write-Ok $h
    } catch {
        # Some Maven roots refuse HEAD on the directory but are reachable.
        if ($_.Exception.Response) { Write-Ok "$h (reachable)" } else { Write-Warn2 "$h unreachable: $($_.Exception.Message)" }
    }
}

if ($missing -gt 0) {
    Write-Host "`n$missing prerequisite(s) missing. Fix the items marked !! and re-run." -ForegroundColor Yellow
    exit 1
}
Write-Host "`nAll set. Next:  powershell -ExecutionPolicy Bypass -File scripts\build.ps1 -Run" -ForegroundColor Green
