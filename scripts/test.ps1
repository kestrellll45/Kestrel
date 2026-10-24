<#
.SYNOPSIS
  Runs formatting, lint and unit tests for the launcher and the client mod.
#>
param([switch]$SkipMod, [switch]$SkipLauncher)
. (Join-Path $PSScriptRoot 'common.ps1')

if (-not $SkipLauncher) {
    if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) { $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path" }
    Write-Step 'cargo fmt --check'
    Invoke-Native 'cargo' @('fmt', '--all', '--check') $LauncherDir
    Write-Step 'cargo clippy (warnings are errors)'
    Invoke-Native 'cargo' @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings') $LauncherDir
    Write-Step 'cargo test'
    Invoke-Native 'cargo' @('test', '--locked') $LauncherDir
    Write-Ok 'launcher checks passed'
}

if (-not $SkipMod) {
    $jdk = Find-GradleJdk
    if (-not $jdk) { throw 'No JDK 17-21 found. Run scripts\setup.ps1 first.' }
    $env:JAVA_HOME = $jdk
    Write-Step 'gradle test'
    Invoke-Native (Join-Path $ClientDir 'gradlew.bat') @('test', '--no-daemon') $ClientDir
    Write-Ok 'client mod tests passed'
}
