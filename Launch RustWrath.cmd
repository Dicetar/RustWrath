@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\launch-client.ps1" -BuildLatest
if errorlevel 1 pause
