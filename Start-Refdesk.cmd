@echo off
setlocal
set "REFDESK_CONFIG=%~dp0.local\settings.json"
set "REFDESK_EXE=%~dp0artifacts\refdesk-tracking.exe"
if not exist "%REFDESK_EXE%" set "REFDESK_EXE=%~dp0artifacts\refdesk-branches.exe"
if not exist "%REFDESK_EXE%" set "REFDESK_EXE=%~dp0artifacts\refdesk.exe"
if not exist "%REFDESK_EXE%" set "REFDESK_EXE=%~dp0target\release\refdesk.exe"
if not exist "%REFDESK_EXE%" (
  echo Build Refdesk with: cargo build --release --locked
  exit /b 1
)
start "" "%REFDESK_EXE%" %*
