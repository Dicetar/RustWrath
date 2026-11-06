@echo off
setlocal
rem Let Windows PowerShell discover its own modules when launched from a newer PowerShell host.
set "PSModulePath="
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\launch-client.ps1" -BuildLatest
if errorlevel 1 pause
