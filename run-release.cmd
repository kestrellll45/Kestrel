@echo off
rem Runs the full release build and saves all output for review.
cd /d "%~dp0"
if not exist dist mkdir dist
if exist dist\release-exit.txt del dist\release-exit.txt
echo Building Kestrel release... output goes to dist\release-console.txt
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\release.ps1 > dist\release-console.txt 2>&1
> dist\release-exit.txt echo %ERRORLEVEL%
echo Done (exit code %ERRORLEVEL%). You can close this window.
