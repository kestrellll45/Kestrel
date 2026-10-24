<#
.SYNOPSIS
  Builds everything and produces dist\Kestrel-<version>-windows-x64.zip (+ .sha256).
#>
param([switch]$SkipTests)
. (Join-Path $PSScriptRoot 'common.ps1')

if (-not $SkipTests) { & (Join-Path $PSScriptRoot 'test.ps1') }
& (Join-Path $PSScriptRoot 'build.ps1') -Configuration release

$version = Get-ModVersion
$zip = Join-Path $DistDir "Kestrel-$version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Write-Step "Packaging $zip"
Compress-Archive -Path (Join-Path $DistDir 'Kestrel\*') -DestinationPath $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
Set-Content -Path "$zip.sha256" -Value "$hash  $(Split-Path $zip -Leaf)" -Encoding ascii
Write-Ok "$zip"
Write-Ok "sha256 $hash"
