<#
.SYNOPSIS
  End-to-end test of the launcher self-updater with real Windows executables.
.DESCRIPTION
  Developer test only. Nothing here is ever published, and it does not touch
  your real %LOCALAPPDATA%\Kestrel or your Kestrel.exe.

  -Mode prepare (default)
    1. Builds the client mod.
    2. Creates a THROW-AWAY test signing key (dist\update-e2e\test-key.txt).
    3. Builds an "old" Kestrel 0.2.0 and a "new" Kestrel 0.2.1, both with the
       test key, a dummy GitHub update source and the e2e-update-test feature
       (which lets KESTREL_E2E_UPDATE_DIR serve the files from a local folder;
       signature, hash, self-test, swap, restart and rollback are all real).
    4. Signs a manifest for 0.2.1 into dist\update-e2e\serve\.
    5. Installs the old exe to dist\update-e2e\install\Kestrel.exe and
       dist\update-e2e\install-crash\Kestrel.exe, with a seeded test data
       folder whose file hashes are recorded.
    6. Writes run-update.cmd / run-update-crash.cmd to start the old exe.

  -Mode verify
    Checks the result of run-update.cmd (new version installed, no leftovers,
    test data untouched) and of run-update-crash.cmd (old version restored).
#>
param([ValidateSet('prepare', 'verify')][string]$Mode = 'prepare')
. (Join-Path $PSScriptRoot 'common.ps1')

$work = Join-Path $DistDir 'update-e2e'
$serve = Join-Path $work 'serve'
$repoUrl = 'https://github.com/kestrel-e2e/kestrel'
$manifestUrl = "$repoUrl/releases/latest/download/kestrel-update.json"
$oldVersion = Get-LauncherVersion
$newVersion = '0.2.1'

function Get-Hashes([string]$dir) {
    $h = @{}
    if (Test-Path $dir) {
        Get-ChildItem $dir -Recurse -File | Where-Object { $_.FullName -notmatch '\\(updates|logs)\\' } | ForEach-Object {
            $h[$_.FullName.Substring($dir.Length)] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash
        }
    }
    return $h
}

function Get-ReportedVersion([string]$exe) {
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('kestrel-e2e-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $report = Join-Path $tmp 'r.txt'
        Start-Process -FilePath $exe -ArgumentList @('--self-check', "`"$report`"", '--root', "`"$(Join-Path $tmp 'data')`"") -Wait -WindowStyle Hidden | Out-Null
        $text = if (Test-Path $report) { Get-Content $report -Raw } else { '' }
        if ($text -match 'PASS  version: Kestrel (\S+) ') { return $Matches[1] }
        return '?'
    } finally { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
}

if ($Mode -eq 'verify') {
    $fail = 0
    function Check([bool]$ok, [string]$what) {
        if ($ok) { Write-Ok $what } else { Write-Host "    FAIL  $what" -ForegroundColor Red; $script:fail++ }
    }
    $state = Get-Content (Join-Path $work 'state.json') -Raw | ConvertFrom-Json

    Write-Step 'Normal update (run-update.cmd)'
    $exe = Join-Path $work 'install\Kestrel.exe'
    Check ((Get-FileHash $exe -Algorithm SHA256).Hash -eq $state.newHash) "install\Kestrel.exe is the signed $newVersion build"
    Check ((Get-ReportedVersion $exe) -eq $newVersion) "installed exe reports version $newVersion"
    Check (-not (Test-Path "$exe.old")) 'previous launcher (Kestrel.exe.old) cleaned up'
    Check (-not (Test-Path "$exe.new")) 'no staged Kestrel.exe.new left'
    $upd = Join-Path $work 'data\updates'
    $left = @(Get-ChildItem $upd -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'Kestrel-*' -or $_.Name -like '*.part' })
    Check ($left.Count -eq 0) 'downloaded update removed from data\updates'
    $now = Get-Hashes (Join-Path $work 'data')
    $same = $true
    foreach ($k in $state.dataHashes.PSObject.Properties.Name) { if ($now[$k] -ne $state.dataHashes.$k) { $same = $false; Write-Host "      changed: $k" } }
    Check $same 'user data (launcher.json, config.json, cosmetics, accounts.dat) unchanged'
    $log = Join-Path $work 'data\logs\launcher.log'
    $prev = Join-Path $work 'data\logs\launcher.prev.log'
    $logs = (Get-Content $log, $prev -Raw -ErrorAction SilentlyContinue) -join "`n"
    Check ($logs -match "updated from $([regex]::Escape($oldVersion)) to $([regex]::Escape($newVersion))") 'new version logged "updated from ... to ..."'

    Write-Step 'Crash after update (run-update-crash.cmd)'
    $cexe = Join-Path $work 'install-crash\Kestrel.exe'
    Check ((Get-FileHash $cexe -Algorithm SHA256).Hash -eq $state.oldHash) "install-crash\Kestrel.exe was rolled back to $oldVersion"
    Check (-not (Test-Path "$cexe.old")) 'no Kestrel.exe.old left after rollback'
    Check (-not (Test-Path "$cexe.failed")) 'no Kestrel.exe.failed left after rollback'
    $clog = (Get-Content (Join-Path $work 'data-crash\logs\launcher.log'), (Join-Path $work 'data-crash\logs\launcher.prev.log') -Raw -ErrorAction SilentlyContinue) -join "`n"
    Check ($clog -match 'rolling back') 'old launcher detected the crash and rolled back'

    Write-Host ''
    if ($fail -eq 0) { Write-Host 'E2E RESULT: PASS' -ForegroundColor Green; exit 0 }
    Write-Host "E2E RESULT: FAIL ($fail check(s))" -ForegroundColor Red; exit 1
}

# ---------------------------------------------------------------- prepare
if (-not (Test-Command 'cargo') -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) { $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path" }
$jdk = Find-GradleJdk
if (-not $jdk) { throw 'No JDK 17-21 found. Run scripts\setup.ps1 first.' }
$env:JAVA_HOME = $jdk

if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force -Path $serve | Out-Null

Write-Step 'Building client mod'
Invoke-Native (Join-Path $ClientDir 'gradlew.bat') @('build', '--no-daemon', '-x', 'test') $ClientDir
$jar = Join-Path $ClientDir "build\libs\kestrel-$(Get-ModVersion).jar"
if (-not (Test-Path $jar)) { throw "mod jar not found: $jar" }

Write-Step 'Creating a throw-away TEST signing key'
Invoke-Native 'cargo' @('build', '--release', '--locked', '-p', 'kestrel-release') $LauncherDir
$tool = Join-Path $LauncherDir 'target\release\kestrel-release.exe'
$key = Join-Path $work 'test-key.txt'
$pub = (& $tool keygen $key).Trim()
if ($LASTEXITCODE -ne 0) { throw 'keygen failed' }

$targetDir = Join-Path $LauncherDir 'target\e2e'
$built = Join-Path $targetDir 'x86_64-pc-windows-msvc\release\kestrel-launcher.exe'
function Build-Launcher([string]$out) {
    $env:KESTREL_EMBED_MOD_JAR = (Resolve-Path $jar).Path
    $env:KESTREL_UPDATE_MANIFEST_URL = $manifestUrl
    $env:KESTREL_UPDATE_PUBLIC_KEY = $pub
    try {
        Invoke-Native 'cargo' @('build', '--release', '-p', 'kestrel-launcher', '--features', 'e2e-update-test',
            '--target', 'x86_64-pc-windows-msvc', '--target-dir', $targetDir) $LauncherDir
    } finally {
        Remove-Item Env:\KESTREL_EMBED_MOD_JAR, Env:\KESTREL_UPDATE_MANIFEST_URL, Env:\KESTREL_UPDATE_PUBLIC_KEY -ErrorAction SilentlyContinue
    }
    Copy-Item $built $out -Force
}

Write-Step "Building OLD launcher $oldVersion (test build)"
$oldExe = Join-Path $work "Kestrel-$oldVersion.exe"
Build-Launcher $oldExe

Write-Step "Building NEW launcher $newVersion (test build)"
$toml = Join-Path $LauncherDir 'Cargo.toml'
$lock = Join-Path $LauncherDir 'Cargo.lock'
$tomlBak = Get-Content $toml -Raw
$lockBak = Get-Content $lock -Raw
try {
    # Build pattern/replacement first: in PowerShell ',' binds tighter than '+'.
    $pattern = '(?m)^(version\s*=\s*)"' + [regex]::Escape($oldVersion) + '"'
    $replacement = '${1}"' + $newVersion + '"'
    $edited = [regex]::Replace($tomlBak, $pattern, $replacement)
    if ($edited -eq $tomlBak) { throw 'could not set the test version in Cargo.toml' }
    [System.IO.File]::WriteAllText($toml, $edited, (New-Object System.Text.UTF8Encoding($false)))
    $newExe = Join-Path $serve 'Kestrel.exe'
    Build-Launcher $newExe
} finally {
    # Restore the real version files exactly.
    [System.IO.File]::WriteAllText($toml, $tomlBak, (New-Object System.Text.UTF8Encoding($false)))
    [System.IO.File]::WriteAllText($lock, $lockBak, (New-Object System.Text.UTF8Encoding($false)))
}

Write-Step "Signing the $newVersion manifest with the TEST key"
$notes = Join-Path $work 'notes.md'
Set-Content -Path $notes -Encoding ascii -Value @"
End-to-end updater test release.
- Downloaded from a local folder (test build only)
- Signature, checksum and self-test are verified as in a real update
"@
Invoke-Native $tool @('sign', '--key', $key, '--exe', $newExe, '--version', $newVersion,
    '--url', "$repoUrl/releases/download/v$newVersion/Kestrel.exe", '--notes', $notes,
    '--manifest-url', $manifestUrl, '--out', (Join-Path $serve 'kestrel-update.json'))
Invoke-Native $tool @('verify', '--manifest', (Join-Path $serve 'kestrel-update.json'), '--pubkey', $pub,
    '--manifest-url', $manifestUrl, '--exe', $newExe)

Write-Step 'Installing the OLD launcher and seeding test data'
foreach ($name in @('install', 'install-crash')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $work $name) | Out-Null
    Copy-Item $oldExe (Join-Path $work "$name\Kestrel.exe")
}
foreach ($d in @('data', 'data-crash')) {
    $root = Join-Path $work $d
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'instance\kestrel\capes') | Out-Null
    Set-Content -Path (Join-Path $root 'launcher.json') -Encoding ascii -Value '{"max_ram_mib": 3072, "offline_name": "E2ETester", "update_auto_check": true}'
    Set-Content -Path (Join-Path $root 'instance\kestrel\config.json') -Encoding ascii -Value '{"cosmetics":{"cape":"aurora"}}'
    Set-Content -Path (Join-Path $root 'accounts.dat') -Encoding ascii -Value 'not-a-real-account'
    Copy-Item (Join-Path $ClientDir 'src\main\resources\assets\kestrel\textures\capes\ember.png') (Join-Path $root 'instance\kestrel\capes\mine.png')
}
$state = [ordered]@{
    oldHash    = (Get-FileHash $oldExe -Algorithm SHA256).Hash
    newHash    = (Get-FileHash $newExe -Algorithm SHA256).Hash
    dataHashes = Get-Hashes (Join-Path $work 'data')
}
$state | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $work 'state.json') -Encoding ascii

$runner = @"
@echo off
rem TEST ONLY: starts the old test launcher; updates come from the serve folder.
set KESTREL_E2E_UPDATE_DIR=$serve
start "" "$work\install\Kestrel.exe" --root "$work\data"
"@
Set-Content -Path (Join-Path $work 'run-update.cmd') -Value $runner -Encoding ascii
$crash = @"
@echo off
rem TEST ONLY: the "new" version simulates a crash on its first start, so the
rem old launcher must roll back.
set KESTREL_E2E_UPDATE_DIR=$serve
set KESTREL_E2E_FAIL_AFTER_UPDATE=1
start "" "$work\install-crash\Kestrel.exe" --root "$work\data-crash"
"@
Set-Content -Path (Join-Path $work 'run-update-crash.cmd') -Value $crash -Encoding ascii

Write-Host ''
Write-Host 'E2E PREPARED' -ForegroundColor Green
Write-Host "  old: $oldExe ($oldVersion)   new: $newExe ($newVersion)"
Write-Host "  1. Run $work\run-update.cmd, click Update Now, wait for the new version to open."
Write-Host "  2. Run $work\run-update-crash.cmd, click Update Now; it must roll back."
Write-Host '  3. powershell -ExecutionPolicy Bypass -File scripts\test-update-e2e.ps1 -Mode verify'
