@echo off
setlocal EnableExtensions DisableDelayedExpansion

cd /d "%~dp0"

rem ============================================================
rem Local configuration
rem ============================================================
set "QT_DIR=C:\Qt\6.8.3\msvc2022_64"
set "BUILD_TYPE=Release"
set "QT_SOURCE=apps\ternary-contours-qt"
set "QT_BUILD=build\qt"

echo.
echo ============================================================
echo  Ternary Contours Qt Builder - MSVC
echo ============================================================
echo  Repository: %CD%
echo  Qt:         %QT_DIR%
echo  Build type: %BUILD_TYPE%
echo.

if not exist "%QT_DIR%\lib\cmake\Qt6\Qt6Config.cmake" (
    echo ERROR: Qt6Config.cmake was not found:
    echo   %QT_DIR%\lib\cmake\Qt6\Qt6Config.cmake
    goto :fail
)

rem ============================================================
rem Locate and initialize Visual Studio 2022 x64 tools.
rem Avoid FOR /F parsing of paths containing "(x86)".
rem ============================================================
set "VCVARS64="

if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" (
    set "VCVARS64=%ProgramFiles%\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
)

if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" (
    set "VCVARS64=%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
)

if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" (
    set "VCVARS64=%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
)

if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    set "VCVARS64=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
)

if not defined VCVARS64 (
    echo ERROR: Visual Studio 2022 C++ tools were not found.
    echo.
    echo Install one of:
    echo   Visual Studio 2022 Community with Desktop development with C++
    echo   Visual Studio 2022 Build Tools with Desktop development with C++
    echo.
    goto :fail
)

echo Initializing MSVC x64 environment:
echo   %VCVARS64%
call "%VCVARS64%"
if errorlevel 1 goto :fail

where cl.exe >nul 2>nul
if errorlevel 1 (
    echo ERROR: cl.exe is unavailable after running vcvars64.bat.
    goto :fail
)

for /f "delims=" %%I in ('where cl.exe') do (
    echo MSVC compiler:
    echo   %%I
    goto :compiler_found
)
:compiler_found

call :require_tool cargo.exe
if errorlevel 1 goto :fail

call :require_tool cmake.exe
if errorlevel 1 goto :fail

call :require_tool ninja.exe
if errorlevel 1 goto :fail

rem ============================================================
rem A CMake build tree cannot switch from MinGW/GNU to MSVC.
rem Remove any existing cache unconditionally for reliability.
rem ============================================================
if exist "%QT_BUILD%\CMakeCache.txt" (
    echo Removing previous CMake cache...
    rmdir /s /q "%QT_BUILD%"
    if exist "%QT_BUILD%" (
        echo ERROR: Could not remove:
        echo   %QT_BUILD%
        goto :fail
    )
)

rem ============================================================
rem Build Rust bridge
rem ============================================================
echo.
echo [1/4] Building Rust Qt bridge...
cargo build -p ternary-contours-qt-bridge --release
if errorlevel 1 goto :fail

set "RUST_BRIDGE=%CD%\target\release\ternary_contours_qt_bridge.lib"
if not exist "%RUST_BRIDGE%" (
    echo ERROR: Rust bridge library was not created:
    echo   %RUST_BRIDGE%
    goto :fail
)

set "RUST_BRIDGE_CMAKE=%RUST_BRIDGE:\=/%"
set "QT6_CMAKE=%QT_DIR%\lib\cmake\Qt6"
set "QT6_CMAKE=%QT6_CMAKE:\=/%"

rem ============================================================
rem Configure with the MSVC environment selected above.
rem ============================================================
echo.
echo [2/4] Configuring Qt project with MSVC...
cmake -S "%QT_SOURCE%" -B "%QT_BUILD%" -G Ninja ^
  -DCMAKE_BUILD_TYPE=%BUILD_TYPE% ^
  -DQt6_DIR="%QT6_CMAKE%" ^
  -DTCQT_RUST_BRIDGE_LIBRARY="%RUST_BRIDGE_CMAKE%"

if errorlevel 1 goto :fail

rem ============================================================
rem Build Qt application
rem ============================================================
echo.
echo [3/4] Building Qt application...
cmake --build "%QT_BUILD%" --config %BUILD_TYPE%
if errorlevel 1 goto :fail

set "QT_EXE=%QT_BUILD%\ternary-contours-qt.exe"
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

rem ============================================================
rem Deploy Qt runtime DLLs
rem ============================================================
echo.
echo [4/4] Deploying Qt runtime files...
if exist "%QT_DIR%\bin\windeployqt.exe" (
    "%QT_DIR%\bin\windeployqt.exe" --release --no-translations "%QT_EXE%"
    if errorlevel 1 (
        echo WARNING: windeployqt reported an error.
    )
) else (
    echo WARNING: windeployqt.exe was not found:
    echo   %QT_DIR%\bin\windeployqt.exe
)

echo.
echo ============================================================
echo  Build completed successfully
echo ============================================================
echo  Executable:
echo    %QT_EXE%
echo.

if not defined NO_LAUNCH (
    echo Launching...
    start "" "%QT_EXE%"
)

endlocal
exit /b 0

:require_tool
where "%~1" >nul 2>nul
if errorlevel 1 (
    echo ERROR: Required tool "%~1" was not found on PATH.
    exit /b 1
)
exit /b 0

:fail
echo.
echo Build or launch failed.
pause
endlocal
exit /b 1
