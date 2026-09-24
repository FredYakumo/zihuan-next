@echo off
rem Build the zihuan_next binary and package a distributable release archive
rem with the same layout as the CI release package (see .github/workflows/build.yml).
rem Pure cmd.exe: no PowerShell required. Zip packing uses the bsdtar shipped
rem with Windows 10 1803+ (`tar -a` picks the format from the .zip extension).
rem
rem Usage: build.bat [cpu|cuda]   (omit the argument for an interactive prompt)
rem Environment overrides:
rem   SKIP_FRONTEND=1  reuse the existing webui\dist instead of running pnpm
rem   SKIP_BUILD=1     repackage the existing target\release binary without cargo
rem
rem The CUDA path mirrors scripts/cargo-cuda.ps1: cl.exe is taken from PATH when
rem available (e.g. inside a "x64 Native Tools Command Prompt"), otherwise it is
rem located via vswhere. It always picks the newest MSVC, so for CUDA versions
rem older than 13.2 prefer the developer prompt / build.ps1, which can pick an
rem MSVC version compatible with the installed CUDA toolkit.
setlocal EnableExtensions EnableDelayedExpansion

set "VARIANT=%~1"
if not defined VARIANT (
    echo zihuan-next: select build variant
    echo   [1] CPU
    echo   [2] CUDA ^(candle-cuda, requires CUDA toolkit + MSVC^)
    set /p CHOICE=Enter choice [1]:
    if "!CHOICE!"=="2" (set "VARIANT=cuda") else (set "VARIANT=cpu")
)
if not "%VARIANT%"=="cpu" if not "%VARIANT%"=="cuda" (
    echo zihuan-next: invalid variant '%VARIANT%' on Windows, allowed: cpu cuda
    exit /b 1
)
echo zihuan-next: variant=%VARIANT%

where cargo >nul 2>nul
if errorlevel 1 (
    echo zihuan-next: cargo not found on PATH. Install Rust via rustup.
    exit /b 1
)
if not "%SKIP_FRONTEND%"=="1" (
    where pnpm >nul 2>nul
    if errorlevel 1 (
        echo zihuan-next: pnpm not found on PATH. Install pnpm ^(e.g. corepack enable^).
        exit /b 1
    )
)
if "%VARIANT%"=="cuda" if not "%SKIP_BUILD%"=="1" (
    where nvcc >nul 2>nul
    if errorlevel 1 (
        echo zihuan-next: nvcc not found on PATH. Install the CUDA toolkit.
        exit /b 1
    )
)

cd /d "%~dp0"

if "%SKIP_FRONTEND%"=="1" (
    if not exist "webui\dist\" (
        echo zihuan-next: webui\dist not found; run without SKIP_FRONTEND first.
        exit /b 1
    )
    echo zihuan-next: skipping webui build ^(SKIP_FRONTEND=1^)
) else (
    echo zihuan-next: building webui
    pushd webui
    call pnpm install --frozen-lockfile || goto :fail_popd
    call pnpm run build || goto :fail_popd
    popd
)

if "%SKIP_BUILD%"=="1" (
    if not exist "target\release\zihuan_next.exe" (
        echo zihuan-next: target\release\zihuan_next.exe not found; run without SKIP_BUILD first.
        exit /b 1
    )
    echo zihuan-next: skipping cargo build ^(SKIP_BUILD=1^)
) else if "%VARIANT%"=="cuda" (
    call :detect_cl || exit /b 1
    echo zihuan-next: building CUDA binary
    cargo build -p zihuan_service --bin zihuan_next --features candle-cuda --release || goto :fail
) else (
    echo zihuan-next: building CPU binary
    cargo build --release -p zihuan_service --bin zihuan_next || goto :fail
)

set "VERSION=dev"
for /f "usebackq tokens=*" %%v in (`git describe --tags --always --dirty 2^>nul`) do set "VERSION=%%v"

set "PACKAGE_ROOT=package\zihuan_next"
set "ARCHIVE_NAME=zihuan_next-%VERSION%-Windows-x86_64-%VARIANT%.zip"
set "ARCHIVE=package\%ARCHIVE_NAME%"
if exist "%PACKAGE_ROOT%" rmdir /s /q "%PACKAGE_ROOT%"
if exist "%ARCHIVE%" del /q "%ARCHIVE%"
mkdir "%PACKAGE_ROOT%\dag_nodes" "%PACKAGE_ROOT%\dynamic_script_engine" "%PACKAGE_ROOT%\sub_agents" "%PACKAGE_ROOT%\scheduled_jobs" || goto :fail
copy /y "target\release\zihuan_next.exe" "%PACKAGE_ROOT%\" >nul || goto :fail
copy /y "package.json" "%PACKAGE_ROOT%\" >nul || goto :fail
xcopy /e /i /y "dag_nodes" "%PACKAGE_ROOT%\dag_nodes" >nul || goto :fail
xcopy /e /i /y "sub_agents" "%PACKAGE_ROOT%\sub_agents" >nul || goto :fail
xcopy /e /i /y "scheduled_jobs" "%PACKAGE_ROOT%\scheduled_jobs" >nul || goto :fail
for /d /r "%PACKAGE_ROOT%" %%d in (__pycache__) do (
    if exist "%%d" rmdir /s /q "%%d"
)
for %%f in (engine.mjs engine_runtime.py zihuan_sdk.mjs zihuan_sdk.py package.json) do (
    copy /y "dynamic_script_engine\%%f" "%PACKAGE_ROOT%\dynamic_script_engine\" >nul || goto :fail
)
copy /y "build_support\release\pyproject.toml" "%PACKAGE_ROOT%\pyproject.toml" >nul || goto :fail

pushd package
rem %SystemRoot%\System32\tar.exe is bsdtar; `tar -a` picks the zip format from the extension.
rem A bare `tar` may resolve to Git Bash's GNU tar, which cannot write .zip.
"%SystemRoot%\System32\tar.exe" -a -cf "%ARCHIVE_NAME%" zihuan_next || goto :fail_popd
popd

echo zihuan-next: package created: %ARCHIVE%
endlocal & exit /b 0

:detect_cl
rem Locate cl.exe: prefer the one already on PATH (developer prompt), then vswhere.
set "CL_PATH="
for /f "usebackq tokens=*" %%i in (`where cl.exe 2^>nul`) do (
    if not defined CL_PATH set "CL_PATH=%%i"
)
if defined CL_PATH (
    echo zihuan-next: using cl.exe from PATH: !CL_PATH!
    exit /b 0
)
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "!VSWHERE!" goto :no_cl
set "VS_PATH="
for /f "usebackq tokens=*" %%i in (`"!VSWHERE!" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VS_PATH=%%i"
if not defined VS_PATH goto :no_cl
set "MSVC_SEL="
for /f "delims=" %%d in ('dir /b /ad /o-n "!VS_PATH!\VC\Tools\MSVC" 2^>nul') do (
    if not defined MSVC_SEL if exist "!VS_PATH!\VC\Tools\MSVC\%%d\bin\Hostx64\x64\cl.exe" set "MSVC_SEL=%%d"
)
if not defined MSVC_SEL goto :no_cl
set "MSVC_ROOT=!VS_PATH!\VC\Tools\MSVC\!MSVC_SEL!"
set "CL_PATH=!MSVC_ROOT!\bin\Hostx64\x64\cl.exe"
set "NVCC_CCBIN=!CL_PATH!"
set "VCToolsInstallDir=!MSVC_ROOT!\"
set "VCINSTALLDIR=!VS_PATH!\VC\"
set "INCLUDE=!MSVC_ROOT!\include;!INCLUDE!"
echo zihuan-next: using cl.exe: !CL_PATH!
exit /b 0

:no_cl
echo zihuan-next: MSVC cl.exe not found. Install Visual Studio Build Tools with the C++ workload,
echo zihuan-next: or run from a "x64 Native Tools Command Prompt".
exit /b 1

:fail_popd
popd
:fail
echo zihuan-next: build failed
exit /b 1
