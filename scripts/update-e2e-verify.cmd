@echo off
rem Developer test of the launcher self-updater (verify step). Output: dist\update-e2e-verify.txt
cd /d "%~dp0.."
if not exist dist mkdir dist
if exist dist\update-e2e-verify-exit.txt del dist\update-e2e-verify-exit.txt
echo Running updater end-to-end test (verify)... output goes to dist\update-e2e-verify.txt
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-update-e2e.ps1 -Mode verify > dist\update-e2e-verify.txt 2>&1
> dist\update-e2e-verify-exit.txt echo %ERRORLEVEL%
echo Done (exit code %ERRORLEVEL%). You can close this window.
