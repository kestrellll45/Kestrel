@echo off
rem Developer test of the launcher self-updater (prepare step). Output: dist\update-e2e-prepare.txt
cd /d "%~dp0.."
if not exist dist mkdir dist
if exist dist\update-e2e-prepare-exit.txt del dist\update-e2e-prepare-exit.txt
echo Running updater end-to-end test (prepare)... output goes to dist\update-e2e-prepare.txt
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-update-e2e.ps1 -Mode prepare > dist\update-e2e-prepare.txt 2>&1
> dist\update-e2e-prepare-exit.txt echo %ERRORLEVEL%
echo Done (exit code %ERRORLEVEL%). You can close this window.
