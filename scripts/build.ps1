<#
.SYNOPSIS
  Builds the Kestrel client mod and launcher and assembles dist\Kestrel.
.PARAMETER Configuration
  release (default) or debug for the launcher.
.PARAMETER SkipMod / SkipLauncher
  Build only one half.
.PARAMETER Run
  Start the launcher from dist\Kestrel when the build succeeds.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build.ps1 -Run
#>
param(
    [ValidateSet('release', 'debug')][string]$Configuration = 'release',
    [switch]$SkipMod,
    [switch]$SkipLauncher,
    [switch]$Run
)
. (Join-Path $PSScriptRoot 'common.ps1')

# Always keep a log, so a window that closes on its own still leaves the output behind.
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
$logFile = Join-Path $DistDir 'build-log.txt'
try { Start-Transcript -Path $logFile -Force | Out-Null } catch { }
trap { Write-Host "BUILD FAILED: $_" -ForegroundColor Red; try { Stop-Transcript | Out-Null } catch { }; break }

$version = Get-ModVersion
$out = Join-Path $DistDir 'Kestrel'
New-Item -ItemType Directory -Force -Path (Join-Path $out 'client') | Out-Null

if (-not $SkipMod) {
    Write-Step "Building client mod $version (Gradle; first run downloads Minecraft, Forge and Java 8)"
    $jdk = Find-GradleJdk
    if (-not $jdk) { throw 'No JDK 17-21 found. Run scripts\setup.ps1 first.' }
    $env:JAVA_HOME = $jdk
    Invoke-Native (Join-Path $ClientDir 'gradlew.bat') @('build', '--no-daemon', '--stacktrace') $ClientDir
    $jar = Join-Path $ClientDir "build\libs\kestrel-$version.jar"
    if (-not (Test-Path $jar)) { throw "expected mod jar not found: $jar" }
    # Replace older bundled jars only.
    Get-ChildItem (Join-Path $out 'client') -Filter 'kestrel-*.jar' -ErrorAction SilentlyContinue | Remove-Item -Force
    Copy-Item $jar (Join-Path $out 'client') -Force
    Write-Ok "mod: $jar"
}

if (-not $SkipLauncher) {
    Write-Step "Building launcher ($Configuration)"
    if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) { $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path" }
    # Embed the freshly built mod so dist\Kestrel\Kestrel.exe is standalone too.
    $modJar = Join-Path $ClientDir "build\libs\kestrel-$version.jar"
    if (Test-Path $modJar) { $env:KESTREL_EMBED_MOD_JAR = (Resolve-Path $modJar).Path }
    $cargoArgs = @('build', '--locked', '-p', 'kestrel-launcher')
    if ($Configuration -eq 'release') { $cargoArgs += '--release' }
    try {
        Invoke-Native 'cargo' $cargoArgs $LauncherDir
    } finally {
        Remove-Item Env:\KESTREL_EMBED_MOD_JAR -ErrorAction SilentlyContinue
    }
    $exe = Join-Path $LauncherDir "target\$Configuration\kestrel-launcher.exe"
    Copy-Item $exe (Join-Path $out 'Kestrel.exe') -Force
    Write-Ok "launcher: $(Join-Path $out 'Kestrel.exe')"
}

Write-Host "`nBuild complete: $out  (log: $logFile)" -ForegroundColor Green
if ($Run) {
    Write-Step 'Starting launcher'
    Start-Process -FilePath (Join-Path $out 'Kestrel.exe') -WorkingDirectory $out
}
try { Stop-Transcript | Out-Null } catch { }
