@echo off
setlocal enabledelayedexpansion

rem --------------------------------------------
rem build_windows_msvc.cmd
rem Build rfvp for Windows using MSVC targets.
rem Output is a plain .exe (no zip).
rem
rem Usage:
rem   build_windows_msvc.cmd x86_64
rem   build_windows_msvc.cmd arm64
rem
rem Optional env vars:
rem   CRATE_PKG=rfvp     (cargo package name)
rem   APP_NAME=rfvp      (output exe name)
rem --------------------------------------------

if "%~1"=="" goto :usage

set "ARCH=%~1"

if /I "%ARCH%"=="x86_64" (
  set "TARGET_TRIPLE=x86_64-pc-windows-msvc"
) else if /I "%ARCH%"=="arm64" (
  set "TARGET_TRIPLE=aarch64-pc-windows-msvc"
) else (
  echo ERROR: unsupported arch "%ARCH%"
  goto :usage
)

if "%CRATE_PKG%"=="" set "CRATE_PKG=rfvp"
if "%APP_NAME%"=="" set "APP_NAME=rfvp"

rem ---- Resolve repo root (script at platform\scripts\) ----
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%\..") do set "PLATFORM_DIR=%%~fI"
for %%I in ("%PLATFORM_DIR%\..") do set "ROOT_DIR=%%~fI"

set "DIST_DIR=%ROOT_DIR%\dist\windows\%ARCH%"
if not exist "%DIST_DIR%" mkdir "%DIST_DIR%"

where cargo >nul 2>nul
if errorlevel 1 (
  echo ERROR: cargo not found in PATH
  exit /b 1
)

where rustup >nul 2>nul
if errorlevel 1 (
  echo ERROR: rustup not found in PATH
  exit /b 1
)

rem ---- Ensure target installed (best-effort) ----
rustup target add %TARGET_TRIPLE% >nul 2>nul

echo [win] Building %CRATE_PKG% (%TARGET_TRIPLE%) ...

pushd "%ROOT_DIR%" >nul
cargo build --release -p "%CRATE_PKG%" --target "%TARGET_TRIPLE%"
if errorlevel 1 (
  popd >nul
  echo ERROR: cargo build failed for %TARGET_TRIPLE%
  exit /b 1
)
popd >nul

set "EXE_IN=%ROOT_DIR%\target\%TARGET_TRIPLE%\release\%CRATE_PKG%.exe"
if not exist "%EXE_IN%" (
  echo ERROR: build output missing: %EXE_IN%
  exit /b 1
)

set "EXE_OUT=%DIST_DIR%\%APP_NAME%.exe"
copy /y "%EXE_IN%" "%EXE_OUT%" >nul

echo [win] OK: %EXE_OUT%
exit /b 0

:usage
echo Usage:
echo   %~nx0 x86_64
echo   %~nx0 arm64
echo.
echo Optional env vars:
echo   set CRATE_PKG=rfvp
echo   set APP_NAME=rfvp
exit /b 2
