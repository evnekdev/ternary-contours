@echo off
setlocal EnableExtensions EnableDelayedExpansion

rem ============================================================
rem ternary-contours Qt build and launcher
rem
rem Optional environment overrides:
rem   set QT_DIR=C:\Qt\6.8.3\msvc2022_64
rem   set BUILD_TYPE=Release
rem   set NO_LAUNCH=1
rem   set CLEAN_BUILD=1
rem ============================================================

cd /d "%~dp0"

if not defined BUILD_TYPE set "BUILD_TYPE=Release"

echo.
echo ============================================================
echo  Ternary Contours Qt Builder
echo ============================================================
echo  Repository: %CD%
echo  Build type: %BUILD_TYPE%
echo.

rem Locate Qt 6
if defined QT_DIR (
    if not exist "%QT_DIR%\lib\cmake\Qt6\Qt6Config.cmake" (
        echo ERROR: QT_DIR is set but Qt6Config.cmake was not found:
        echo   %QT_DIR%\lib\cmake\Qt6\Qt6Config.cmake
        goto :fail
    )
) else (
    set "QT_DIR="
    for /f "delims=" %%D in ('dir /b /ad /o-n "C:\Qt\6.*" 2^>nul') do (
        if exist "C:\Qt\%%D\msvc2022_64\lib\cmake\Qt6\Qt6Config.cmake" (
            set "QT_DIR=C:\Qt\%%D\msvc2022_64"
            goto :qt_found
        )
        if exist "C:\Qt\%%D\msvc2019_64\lib\cmake\Qt6\Qt6Config.cmake" (
            set "QT_DIR=C:\Qt\%%D\msvc2019_64"
            goto :qt_found
        )
    )
)

:qt_found
if not defined QT_DIR (
    echo ERROR: Qt 6 could not be found automatically.
    echo.
    echo Set QT_DIR before running this script, for example:
    echo   set QT_DIR=C:\Qt\6.8.3\msvc2022_64
    echo.
    goto :fail
)

echo Using Qt:
echo   %QT_DIR%
echo.

rem Initialize Visual Studio MSVC environment if needed
where cl >nul 2>nul
if errorlevel 1 (
    set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"

    if not exist "!VSWHERE!" (
        echo ERROR: MSVC compiler was not found and vswhere.exe is unavailable.
        echo Run this script from Developer Command Prompt for VS 2022,
        echo or install Visual Studio Build Tools with Desktop C++ support.
        goto :fail
    )

    for /f "usebackq tokens=*" %%I in (`"!VSWHERE!" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do (
        set "VS_INSTALL=%%I"
    )

    if not defined VS_INSTALL (
        echo ERROR: Visual Studio C++ build tools were not found.
        goto :fail
    )

    echo Initializing MSVC environment...
    call "!VS_INSTALL!\VC\Auxiliary\Build\vcvars64.bat"
    if errorlevel 1 goto :fail
)

call :require_tool cargo
if errorlevel 1 goto :fail
call :require_tool cmake
if errorlevel 1 goto :fail
call :require_tool ninja
if errorlevel 1 goto :fail

set "RUST_BRIDGE=target\release\ternary_contours_qt_bridge.lib"
set "QT_SOURCE=apps\ternary-contours-qt"
set "QT_BUILD=build\qt"
set "QT_EXE=%QT_BUILD%\ternary-contours-qt.exe"

if /i "%BUILD_TYPE%"=="Debug" (
    set "RUST_PROFILE="
    set "RUST_BRIDGE=target\debug\ternary_contours_qt_bridge.lib"
) else (
    set "RUST_PROFILE=--release"
)

if defined CLEAN_BUILD (
    echo Removing previous Qt build directory...
    if exist "%QT_BUILD%" rmdir /s /q "%QT_BUILD%"
)

echo.
echo [1/4] Building Rust Qt bridge...
cargo build -p ternary-contours-qt-bridge %RUST_PROFILE%
if errorlevel 1 (
    echo ERROR: Rust bridge build failed.
    goto :fail
)

if not exist "%RUST_BRIDGE%" (
    echo ERROR: Expected Rust bridge library was not created:
    echo   %RUST_BRIDGE%
    goto :fail
)

for %%I in ("%RUST_BRIDGE%") do set "RUST_BRIDGE_ABS=%%~fI"
set "RUST_BRIDGE_CMAKE=!RUST_BRIDGE_ABS:\=/!"
set "QT6_CMAKE=%QT_DIR%\lib\cmake\Qt6"
set "QT6_CMAKE=!QT6_CMAKE:\=/!"

echo.
echo [2/4] Configuring Qt project...
cmake -S "%QT_SOURCE%" -B "%QT_BUILD%" -G Ninja ^
  -DCMAKE_BUILD_TYPE=%BUILD_TYPE% ^
  -DQt6_DIR="!QT6_CMAKE!" ^
  -DTCQT_RUST_BRIDGE_LIBRARY="!RUST_BRIDGE_CMAKE!"
if errorlevel 1 (
    echo ERROR: CMake configuration failed.
    goto :fail
)

echo.
echo [3/4] Building Qt application...
cmake --build "%QT_BUILD%" --config %BUILD_TYPE%
if errorlevel 1 (
    echo ERROR: Qt application build failed.
    goto :fail
)

if not exist "%QT_EXE%" (
    if exist "%QT_BUILD%\%BUILD_TYPE%\ternary-contours-qt.exe" (
        set "QT_EXE=%QT_BUILD%\%BUILD_TYPE%\ternary-contours-qt.exe"
    ) else (
        echo ERROR: Built executable was not found.
        echo Checked:
        echo   %QT_BUILD%\ternary-contours-qt.exe
        echo   %QT_BUILD%\%BUILD_TYPE%\ternary-contours-qt.exe
        goto :fail
    )
)

echo.
echo [4/4] Deploying Qt runtime files...
if exist "%QT_DIR%\bin\windeployqt.exe" (
    "%QT_DIR%\bin\windeployqt.exe" --release --no-translations "%QT_EXE%"
    if errorlevel 1 (
        echo WARNING: windeployqt reported an error.
        echo The executable may still run if Qt is already on PATH.
    )
) else (
    echo WARNING: windeployqt.exe was not found under:
    echo   %QT_DIR%\bin
)

echo.
echo ============================================================
echo  Build completed successfully
echo ============================================================
echo  Executable:
echo    %QT_EXE%
echo.

if defined NO_LAUNCH (
    echo NO_LAUNCH is set. The application was not started.
    goto :success
)

echo Launching application...
start "" "%QT_EXE%"
goto :success

:require_tool
where %~1 >nul 2>nul
if errorlevel 1 (
    echo ERROR: Required tool "%~1" was not found on PATH.
    exit /b 1
)
exit /b 0

:fail
echo.
echo Build or launch failed.
pause
exit /b 1

:success
endlocal
exit /b 0
