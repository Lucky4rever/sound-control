@echo off
setlocal enabledelayedexpansion

:: Перехід у папку скрипта (рут проєкту)
cd /d "%~dp0"

set CARGO_FILE=crates\gui\Cargo.toml
set EXE_NAME=sound-control
set PLATFORM=windows

:: --- Читання версії з Cargo.toml ---
for /f "usebackq tokens=1,2 delims==" %%a in (`findstr /B "version" %CARGO_FILE%`) do (
    set RAW_VERSION=%%b
    goto :got_ver
)
:got_ver
set VERSION=%RAW_VERSION:"=%
set VERSION=%VERSION: =%

echo ===========================================
echo  Packaging %EXE_NAME% v%VERSION% (%PLATFORM%)
echo ===========================================

:: --- Білд ---
echo [1/3] Building release...
cargo build --release -p %EXE_NAME%
if %ERRORLEVEL% neq 0 (
    echo ERROR: Build failed!
    pause
    exit /b 1
)

:: --- Підготовка тимчасової папки ---
set TEMP_DIR=target\package-%PLATFORM%-temp
set OUT_FILE=target\%EXE_NAME%-v%VERSION%-%PLATFORM%.zip

if exist "%TEMP_DIR%" rmdir /S /Q "%TEMP_DIR%"
mkdir "%TEMP_DIR%\assets"

:: --- Копіювання файлів ---
echo [2/3] Collecting files...
copy "target\release\%EXE_NAME%.exe" "%TEMP_DIR%\"
if exist "assets" xcopy /E /I /Q "assets\*" "%TEMP_DIR%\assets\"
if exist "README.md" copy "README.md" "%TEMP_DIR%\"
if exist "LICENSE" copy "LICENSE" "%TEMP_DIR%\"

:: --- Архівація ---
echo [3/3] Archiving...
powershell -NoProfile -Command "Compress-Archive -Path '%TEMP_DIR%\*' -DestinationPath '%OUT_FILE%' -Force"

:: --- Очистка ---
rmdir /S /Q "%TEMP_DIR%"

echo.
echo Done: %OUT_FILE%
pause