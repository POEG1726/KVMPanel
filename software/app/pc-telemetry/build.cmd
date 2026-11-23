@echo off
setlocal
where cl.exe >nul 2>nul || (
  echo Run build.cmd in the x64 Native Tools Command Prompt for Visual Studio.
  exit /b 1
)
set "out=%~dp0..\..\..\build\pc-telemetry"
if not exist "%out%" mkdir "%out%" || exit /b 1
pushd "%out%" || exit /b 1
cl /nologo /std:c17 /utf-8 /W4 /WX /O2 /MT /DUNICODE /D_UNICODE /D_WIN32_WINNT=0x0A00 ^
  "%~dp0main.c" "%~dp0metrics.c" "%~dp0sources.c" "%~dp0pawn.c" "%~dp0hotspot.c" ^
  "%~dp0ryzen.c" "%~dp0ddr5.c" "%~dp0radeon.c" "%~dp0usb.c" ^
  /Fe:KVMPanelTelemetry.exe /link /SUBSYSTEM:WINDOWS user32.lib gdi32.lib shell32.lib ^
  advapi32.lib iphlpapi.lib pdh.lib hid.lib setupapi.lib wlanapi.lib
set "result=%errorlevel%"
popd
exit /b %result%
