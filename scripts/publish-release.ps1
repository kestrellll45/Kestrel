<#
.SYNOPSIS
  Builds, signs and publishes a Kestrel release on GitHub.
.DESCRIPTION
  Refuses to publish unless every step succeeds:
  1. Signing key present and matching launcher\update-public-key.txt.
  2. Git checkout on branch main, no uncommitted changes, and HEAD pushed
     to origin/main (the release tag points at exactly the source that was built).
  3. Version not already released.
  4. scripts\release.ps1: tests, client mod, launcher, self-check (which also
     confirms the exe embeds this repository and this public key).
  5. Signs release\kestrel-update.json and verifies it against the public key
     compiled into the launcher and against the exe's SHA-256.
  6. Creates GitHub release v<version> (marked Latest) with Kestrel.exe and
     kestrel-update.json via the GitHub CLI (gh).
  Installed launchers see the update the next time they start.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\publish-release.ps1 -Notes notes\0.2.1.md
#>
param(
    [Parameter(Mandatory)][string]$Notes,
    [string]$MinimumSupported = '',
    [switch]$SkipTests,
    [switch]$Draft
)
. (Join-Path $PSScriptRoot 'common.ps1')

# Portable Git / GitHub CLI installed for the current user (dist\tasks setup).
foreach ($d in @("$env:LOCALAPPDATA\Programs\PortableGit\cmd", "$env:LOCALAPPDATA\Programs\GitHubCLI\bin", "$env:USERPROFILE\.cargo\bin")) {
    if ((Test-Path $d) -and (($env:Path -split ';') -notcontains $d)) { $env:Path = "$d;$env:Path" }
}

$update = Get-UpdateConfig
if (-not $update) { throw 'Updates are not configured. Run scripts\setup-updates.ps1 -Repo owner/repo first.' }
$keyPath = Get-SigningKeyPath
if (-not (Test-Path $keyPath)) { throw "Signing key not found at $keyPath (restore your backup, or set KESTREL_SIGNING_KEY)." }
if (-not (Test-Path $Notes)) { throw "Release notes file not found: $Notes" }
$notesPath = (Resolve-Path $Notes).Path
if (-not (Test-Command 'gh')) { throw 'GitHub CLI (gh) not found. Run dist\tasks\2-install-github-tools.cmd or install it from https://cli.github.com/' }
if (-not (Test-Command 'git')) { throw 'git not found.' }
$version = Get-LauncherVersion
$tag = "v$version"

# 1. The private key must belong to the public key that launchers trust.
Write-Step 'Checking the signing key'
Invoke-Native 'cargo' @('build', '--release', '--locked', '-p', 'kestrel-release') $LauncherDir
$tool = Join-Path $LauncherDir 'target\release\kestrel-release.exe'
$signingPublic = (& $tool pubkey $keyPath | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "cannot read signing key $keyPath" }
if ($signingPublic -ne $update.PublicKey) {
    throw "the signing key $keyPath (key $(Get-KeyId $signingPublic)) does not match launcher\update-public-key.txt (key $($update.KeyId)). Installed launchers would reject this release."
}
Write-Ok "signing key matches the trusted public key (key $($update.KeyId))"

# 2. Publish only a clean, pushed main.
function Assert-CleanPushedMain {
    $branch = Invoke-Git @('rev-parse', '--abbrev-ref', 'HEAD')
    if ($branch -ne 'main') { throw "releases are published from branch main (current: $branch)" }
    $dirty = Invoke-Git @('status', '--porcelain')
    if ($dirty) { throw "uncommitted changes - commit and push them first:`n$dirty" }
    Invoke-Git @('fetch', '--quiet', 'origin', 'main') | Out-Null
    $head = Invoke-Git @('rev-parse', 'HEAD')
    $remote = Invoke-Git @('rev-parse', 'origin/main')
    if ($head -ne $remote) { throw "HEAD ($head) is not what origin/main has ($remote) - push (or pull) first" }
    return $head
}
Write-Step 'Checking the git checkout'
$commit = Assert-CleanPushedMain
Write-Ok "main @ $commit is committed and pushed"

# 3. Never overwrite an existing release.
$prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
& gh auth status --hostname github.com *> $null
$signedIn = ($LASTEXITCODE -eq 0)
& gh release view $tag --repo $update.Repo *> $null
$exists = ($LASTEXITCODE -eq 0)
$ErrorActionPreference = $prev
if (-not $signedIn) { throw 'GitHub CLI is not signed in. Run: gh auth login' }
if ($exists) { throw "Release $tag already exists in $($update.Repo). Bump the version with scripts\bump-version.ps1." }

# 4. Build and test.
$releaseArgs = @()
if ($SkipTests) { $releaseArgs += '-SkipTests' }
& (Join-Path $PSScriptRoot 'release.ps1') @releaseArgs
$exe = Join-Path $Root 'release\Kestrel.exe'
if (-not (Test-Path $exe)) { throw 'release.ps1 did not produce release\Kestrel.exe' }
# The build must not have changed tracked files (e.g. Cargo.lock).
if ((Assert-CleanPushedMain) -ne $commit) { throw 'the checkout changed during the build' }

# 5. Sign and verify.
Write-Step "Signing manifest for $version"
$manifest = Join-Path $Root 'release\kestrel-update.json'
$url = "https://github.com/$($update.Repo)/releases/download/$tag/Kestrel.exe"
$published = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
$signArgs = @('sign', '--key', $keyPath, '--exe', $exe, '--version', $version, '--url', $url,
    '--notes', $notesPath, '--published', $published, '--manifest-url', $update.ManifestUrl, '--out', $manifest)
if ($MinimumSupported) { $signArgs += @('--minimum', $MinimumSupported) }
Invoke-Native $tool $signArgs
Invoke-Native $tool @('verify', '--manifest', $manifest, '--pubkey', $update.PublicKey, '--manifest-url', $update.ManifestUrl, '--exe', $exe)
Write-Ok 'manifest signed and verified with the public key built into the launcher'

# 6. Publish.
Write-Step "Creating GitHub release $tag in $($update.Repo) at $commit"
$ghArgs = @('release', 'create', $tag, $exe, $manifest, '--repo', $update.Repo, '--target', $commit,
    '--title', "Kestrel $version", '--notes-file', $notesPath)
# Must become the "Latest" release: launchers read releases/latest.
if ($Draft) { $ghArgs += '--draft' } else { $ghArgs += '--latest' }
Invoke-Native 'gh' $ghArgs
Write-Ok "published ${tag}: https://github.com/$($update.Repo)/releases/tag/$tag"
Write-Ok "manifest: $($update.ManifestUrl)"
Write-Host 'Installed launchers will offer this version on their next start.'
