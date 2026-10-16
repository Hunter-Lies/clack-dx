@echo off
rem ============================================================
rem  Clack DX - test helper: wipe or damage the DirectX runtimes
rem
rem  Modes:
rem    clean_dx.cmd list   = list what would be touched (nothing changed)
rem    clean_dx.cmd clean  = delete every DX dll Clack DX would install
rem    clean_dx.cmd break  = damage a few of them, to test the
rem                          "present but wrong" detection + repair
rem    double-click        = clean (asks for admin)
rem
rem  Covered: d3dx9_ d3dx10 d3dx11 d3dcompiler_ d3dcsx_ xaudio2_
rem           xactengine xapofx x3daudio xinput*
rem  Never touched: xinput9_1_0.dll - that one belongs to Windows
rem  (Clack DX whitelists it too).
rem
rem  NOTE: keep this file ASCII-only. A .cmd with non-ASCII text breaks
rem  under some console code pages (a GBK trail byte can be '|' or '\',
rem  which splits lines mid-command).
rem ============================================================
setlocal EnableExtensions EnableDelayedExpansion
set "SELF=%~f0"
set "MODE=%~1"
if /i "%MODE%"=="" set "MODE=clean"
if /i not "%MODE%"=="list" if /i not "%MODE%"=="clean" if /i not "%MODE%"=="break" (
    echo Usage: clean_dx.cmd [list^|clean^|break]
    exit /b 2
)

if /i "%MODE%"=="list" goto :RUN

rem ---- needs admin: relaunch ourselves elevated ----
net session >nul 2>&1
if errorlevel 1 (
    echo.
    echo Asking for administrator rights...
    powershell -NoProfile -Command "Start-Process -FilePath '%SELF%' -ArgumentList '%MODE%' -Verb RunAs" >nul 2>&1
    if errorlevel 1 echo Elevation was cancelled, nothing was changed.
    exit /b
)

:RUN
title Clack DX - %MODE%
set "LOG=%~dp0clean_dx.log"
set "FAILED=0"
set "COUNT=0"

if /i not "%MODE%"=="list" (
    echo ============================================================ > "%LOG%"
    echo Clack DX %MODE%  %DATE% %TIME% >> "%LOG%"
    echo ============================================================ >> "%LOG%"
)

echo.
echo ============================================================
if /i "%MODE%"=="list"  echo   Clack DX - list mode (nothing is changed)
if /i "%MODE%"=="clean" echo   Clack DX - deleting DirectX runtime dlls
if /i "%MODE%"=="break" echo   Clack DX - damaging a few of them
echo ============================================================
echo.

rem ---- what we own ----
rem ? cmd ???????????? findstr ???`/c:` ?????????
rem ?????`^xinput` ?? chxinputrouter.dll ???????????
set "PREFIXES=d3dx9_ d3dx10 d3dx11 d3dcompiler_ d3dcsx_ xaudio2_ xactengine xapofx x3daudio xinput"
rem Windows' own, never delete: Clack DX whitelists xinput9_1_0 (a stub that
rem follows the OS), and 1_4 / Uap ship with Windows 8+ - they are not in the
rem June 2010 redist, so Clack DX could not put them back either.
set "KEEP= xinput9_1_0.dll xinput1_4.dll xinputuap.dll "

for %%D in ("%SystemRoot%\System32" "%SystemRoot%\SysWOW64") do (
    if exist "%%~D" (
        for %%P in (%PREFIXES%) do (
            for %%F in ("%%~D\%%P*.dll") do (
                if exist "%%~F" call :ONE "%%~D" "%%~nxF"
            )
        )
    )
)

if "%MODE%"=="break" call :DAMAGE

echo.
echo ============================================================
echo   %COUNT% file(s) handled, %FAILED% failed
if /i not "%MODE%"=="list" echo   Log: %LOG%
echo.
if /i "%MODE%"=="clean" echo   Now run Clack DX: it should report them missing.
if /i "%MODE%"=="break" echo   Now run Clack DX: it should report "present but wrong".
echo.
pause
exit /b


rem ================= helpers =================

:ONE
set "DIR=%~1"
set "NAME=%~2"
rem ??????"??"?????????
echo !KEEP! | findstr /i /c:" %NAME% " >nul 2>&1
if not errorlevel 1 exit /b
set /a COUNT+=1
if /i "%MODE%"=="list" (
    echo    %~1\%~2
    exit /b
)
if /i "%MODE%"=="break" exit /b

del /f /q "%~1\%~2" >nul 2>&1
if exist "%~1\%~2" (
    echo    in use, cannot delete: %~2
    echo    in use, cannot delete: %~1\%~2 >> "%LOG%"
    set /a FAILED+=1
) else (
    echo    deleted %~2
    echo    deleted %~1\%~2 >> "%LOG%"
)
exit /b


:DAMAGE
rem Break a handful of files so the detector has something to find.
rem Two kinds: zero-byte / truncated (-> unreadable version), and a
rem downgrade (older file copied over a newer one).
echo.
echo Damaging a few files...
for %%D in ("%SystemRoot%\System32" "%SystemRoot%\SysWOW64") do (
    if exist "%%~D" (
        rem 0 bytes: "content is gone"
        for %%N in (d3dx9_43.dll d3dx9_42.dll xaudio2_7.dll d3dcompiler_47.dll) do (
            if exist "%%~D\%%N" (
                copy /y nul "%%~D\%%N" >nul 2>&1
                echo    zeroed %%~D\%%N
                echo    zeroed %%~D\%%N >> "%LOG%"
            )
        )
        rem downgraded: 9.18.904.15 copied over 9.29.952.3111
        if exist "%%~D\d3dx9_24.dll" if exist "%%~D\d3dx9_43.dll" (
            copy /y "%%~D\d3dx9_24.dll" "%%~D\d3dx9_43.dll" >nul 2>&1
            echo    downgraded %%~D\d3dx9_43.dll (from d3dx9_24.dll)
            echo    downgraded %%~D\d3dx9_43.dll >> "%LOG%"
        )
    )
)
echo.
echo A reboot may be needed before the damaged files are really readable.
exit /b
