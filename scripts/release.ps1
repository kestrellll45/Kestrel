<#
.SYNOPSIS
  Produces the standalone release: release\Kestrel.exe (one file).
.DESCRIPTION
  1. Runs the launcher and mod tests (skip with -SkipTests).
  2. Builds the Forge client mod (Gradle).
  3. Builds the launcher in release mode with the mod jar, icon and all
     launcher resources embedded, statically linked (no VC++ redistributable).
  4. Copies the exe to a temporary folder outside the project and runs its
     built-in self-check there with a throw-away data directory.
  5. Checks the exe's DLL imports (when dumpbin is available).
  6. Leaves only Kestrel.exe (+ a SHA-256 text file) in release\.
  Minecraft, Forge and Java 8 are NOT bundled: Kestrel downloads them from the
  official servers into %LOCALAPPDATA%\Kestrel on first launch.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\release.ps1
#>
param([switch]$SkipTests)
. (Join-Path $PSScriptRoot 'common.ps1')

$ReleaseDir = Join-Path $Root 'release'
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
$logFile = Join-Path $DistDir 'release-log.txt'
try { Start-Transcript -Path $logFile -Force | Out-Null } catch { }
trap { Write-Host "RELEASE FAILED: $_" -ForegroundColor Red; try { Stop-Transcript | Out-Null } catch { }; break }

if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) { $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path" }
$jdk = Find-GradleJdk
if (-not $jdk) { throw 'No JDK 17-21 found. Run scripts\setup.ps1 first.' }
$env:JAVA_HOME = $jdk
$version = Get-ModVersion
$launcherVersion = Get-LauncherVersion
if ($version -ne $launcherVersion) {
    throw "Version mismatch: launcher $launcherVersion vs client mod $version. Use scripts\bump-version.ps1 to change both."
}
$update = Get-UpdateConfig
if ($update) {
    Write-Ok "update source: $($update.Repo) (signed releases)"
} else {
    Write-Warn2 'updates not configured (run scripts\setup-updates.ps1); this build will not be able to update itself'
}

# 1. Tests
if (-not $SkipTests) {
    & (Join-Path $PSScriptRoot 'test.ps1')
}

# 2. Client mod
Write-Step "Building client mod $version"
Invoke-Native (Join-Path $ClientDir 'gradlew.bat') @('build', '--no-daemon') $ClientDir
$jar = Join-Path $ClientDir "build\libs\kestrel-$version.jar"
if (-not (Test-Path $jar)) { throw "expected mod jar not found: $jar" }
Write-Ok "$jar ($([math]::Round((Get-Item $jar).Length / 1KB)) KB)"

# 3. Launcher with everything embedded
Write-Step 'Building Kestrel.exe (release, mod + resources embedded, static CRT)'
$env:KESTREL_EMBED_MOD_JAR = (Resolve-Path $jar).Path
if ($update) {
    $env:KESTREL_UPDATE_MANIFEST_URL = $update.ManifestUrl
    $env:KESTREL_UPDATE_PUBLIC_KEY = $update.PublicKey
}
try {
    Invoke-Native 'cargo' @('build', '--release', '--locked', '-p', 'kestrel-launcher', '--target', 'x86_64-pc-windows-msvc') $LauncherDir
} finally {
    Remove-Item Env:\KESTREL_EMBED_MOD_JAR, Env:\KESTREL_UPDATE_MANIFEST_URL, Env:\KESTREL_UPDATE_PUBLIC_KEY -ErrorAction SilentlyContinue
}
$built = Join-Path $LauncherDir 'target\x86_64-pc-windows-msvc\release\kestrel-launcher.exe'
if (-not (Test-Path $built)) { throw "launcher build output not found: $built" }

# 4. Self-check from outside the project, with a fresh data directory
Write-Step 'Self-check of the packaged exe outside the project folder'
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("kestrel-release-check-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    Copy-Item $built (Join-Path $tmp 'Kestrel.exe')
    $report = Join-Path $tmp 'self-check.txt'
    $p = Start-Process -FilePath (Join-Path $tmp 'Kestrel.exe') -ArgumentList @('--self-check', "`"$report`"", '--root', "`"$(Join-Path $tmp 'data')`"") -WorkingDirectory $tmp -Wait -PassThru -WindowStyle Hidden
    if (Test-Path $report) { Get-Content $report | ForEach-Object { Write-Host "    $_" } }
    if ($p.ExitCode -ne 0) { throw "self-check failed (exit code $($p.ExitCode))" }
    $reportText = if (Test-Path $report) { Get-Content $report -Raw } else { '' }
    if ($reportText -notmatch "PASS  version: Kestrel $([regex]::Escape($version)) ") {
        throw "self-check does not report version $version"
    }
    # The exe must contain exactly the update source configured in this checkout,
    # otherwise a published release could never be updated again.
    if ($update -and $reportText -notmatch "PASS  update source: github\.com/$([regex]::Escape($update.Repo)) ") {
        throw "the built exe does not contain the update source $($update.Repo)"
    }
    # ...and exactly the trusted public key from launcher\update-public-key.txt.
    if ($update -and $reportText -notmatch "\(key $([regex]::Escape($update.KeyId))\)") {
        throw "the built exe does not contain the public key $($update.KeyId) from launcher\update-public-key.txt"
    }
    if (-not $update -and $reportText -notmatch 'update source: not configured') {
        throw 'the built exe unexpectedly contains an update source'
    }
    # Test-only update hooks (cargo feature e2e-update-test) must never ship.
    if ($reportText -notmatch 'PASS  update test hooks: none') {
        throw 'the built exe contains update TEST hooks; it must not be distributed'
    }
    Write-Ok 'self-check passed'
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# 5. Dependencies: only DLLs that ship with Windows
$vswhere = Find-VsWhere
if ($vswhere) {
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $dumpbin = Get-ChildItem -Path "$vs\VC\Tools\MSVC" -Recurse -Filter dumpbin.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match 'Hostx64\\x64' } | Select-Object -First 1
    if ($dumpbin) {
        Write-Step 'Checking DLL imports'
        $deps = & $dumpbin.FullName /nologo /dependents $built | Where-Object { $_ -match '^\s+\S+\.dll\s*$' } | ForEach-Object { $_.Trim() }
        $bad = $deps | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt)' }
        Write-Ok ("imports: " + ($deps -join ', '))
        if ($bad) { throw "Kestrel.exe still needs the Visual C++ runtime: $($bad -join ', ')" }
        Write-Ok 'no Visual C++ redistributable required'
    }
}

# 6. Clean release folder
Write-Step 'Writing release folder'
if (Test-Path $ReleaseDir) { Remove-Item -Recurse -Force $ReleaseDir }
New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
$exe = Join-Path $ReleaseDir 'Kestrel.exe'
Copy-Item $built $exe
$hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
Set-Content -Path (Join-Path $ReleaseDir 'Kestrel.exe.sha256.txt') -Value "$hash  Kestrel.exe" -Encoding ascii
$sizeMb = [math]::Round((Get-Item $exe).Length / 1MB, 2)

Write-Host ''
Write-Host "Release ready:" -ForegroundColor Green
Write-Host "  Version: $version"
Write-Host "  File:    $exe"
Write-Host "  Size:    $sizeMb MB"
Write-Host "  SHA-256: $hash"
Write-Host "  Updates: $(if ($update) { "enabled ($($update.Repo), key $($update.KeyId))" } else { 'not configured' })"
Write-Host "  Log:     $logFile"
Write-Host "Copy Kestrel.exe anywhere (Desktop, another PC) and run it. Game files go to %LOCALAPPDATA%\Kestrel."
try { Stop-Transcript | Out-Null } catch { }
