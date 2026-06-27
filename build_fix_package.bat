@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo ═══════════════════════════════════════════════════════════════
echo   Windows 7 兼容版 - 构建 + PE修复 + 打包
echo ═══════════════════════════════════════════════════════════════
echo.

:: ============================================================
:: [1/5] Clean build
:: ============================================================
echo [1/5] Cleaning old build artifacts...
rmdir /s /q "target\release" 2>nul
if exist "target\release\xingda-jiesuan.exe" (
    del /f "target\release\xingda-jiesuan.exe" 2>nul
)
echo        Done.

echo.
echo [2/5] Building release (this takes ~3-5 minutes)...
cargo build --release
if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] Build failed! Check above for errors.
    pause
    exit /b 1
)
echo        Build succeeded.

:: ============================================================
:: [3/5] Fix PE import table for Windows 7
:: ============================================================
echo.
echo [3/5] Applying Windows 7 PE import fix...
set "EXE_PATH=target\release\xingda-jiesuan.exe"
if not exist "%EXE_PATH%" (
    echo [ERROR] %EXE_PATH% not found!
    pause
    exit /b 1
)

python fix_win7_import.py "%EXE_PATH%"
if %ERRORLEVEL% neq 0 (
    echo [WARN] PE fix returned non-zero exit code
    echo        The exe may still work on Win8+, but might fail on Win7.
    echo        If you see "GetSystemTimePreciseAsFileTime" errors on Win7,
    echo        run: pip install pefile
    echo        then: python fix_win7_import.py target\release\xingda-jiesuan.exe
)

:: ============================================================
:: [4/5] Verify
:: ============================================================
echo.
echo [4/5] Verifying PE imports...

:: Find dumpbin
set "DUMPBIN="
for /d %%i in (D:\Microsoft\VS2026Insiders\VC\Tools\MSVC\*) do (
    if exist "%%i\bin\HostX64\x64\dumpbin.exe" (
        set "DUMPBIN=%%i\bin\HostX64\x64\dumpbin.exe"
    )
)
if not defined DUMPBIN (
    for /d %%i in (C:\Microsoft\VS*\VC\Tools\MSVC\*) do (
        if exist "%%i\bin\HostX64\x64\dumpbin.exe" (
            set "DUMPBIN=%%i\bin\HostX64\x64\dumpbin.exe"
        )
    )
)

if defined DUMPBIN (
    echo --- PE Subsystem Info ---
    "%DUMPBIN%" /headers "%EXE_PATH%" | findstr /i "subsystem"
    echo.
    echo --- kernel32 GetSystemTime imports ---
    "%DUMPBIN%" /imports "%EXE_PATH%" | findstr /i "GetSystemTime"
    echo.
    echo Expected: Both lines show GetSystemTimeAsFileTime (no PreciseAsFileTime^)
    echo.
) else (
    echo [WARN] dumpbin.exe not found, skipping verification
    echo        Install Visual Studio Build Tools for dumpbin.
)

:: ============================================================
:: [5/5] Package for distribution
:: ============================================================
echo [5/5] Packaging for distribution...
echo.

set "DIST_DIR=LianTieJieSuan"
set "ZIP_NAME=LianTieJieSuan"

:: Copy exe
echo        Copying xingda-jiesuan.exe to %DIST_DIR%/ ...
copy /Y "%EXE_PATH%" "%DIST_DIR%\" >nul
echo        Done.

:: Copy rules
echo        Copying classify_rules.yaml to %DIST_DIR%/ ...
if exist "classify_rules.yaml" (
    copy /Y "classify_rules.yaml" "%DIST_DIR%\" >nul
    echo        Done.
) else (
    echo        [WARN] classify_rules.yaml not found (exe has built-in rules^)
)

:: Create ZIP
set "ZIP7="
if exist "C:\Program Files\7-Zip\7z.exe" set "ZIP7=C:\Program Files\7-Zip\7z.exe"
if exist "C:\Program Files (x86)\7-Zip\7z.exe" set "ZIP7=C:\Program Files (x86)\7-Zip\7z.exe"

if defined ZIP7 (
    echo        Creating %ZIP_NAME%.zip ...
    del "%ZIP_NAME%.zip" 2>nul
    "%ZIP7%" a -tzip "%ZIP_NAME%.zip" "%DIST_DIR%\*" -xr!"desktop.ini" -xr!".gitkeep" >nul
    if %errorlevel% equ 0 (
        echo        Created: %ZIP_NAME%.zip
    ) else (
        echo        [WARN] 7-Zip failed, folder is still ready in %DIST_DIR%/
    )
) else (
    echo        [INFO] 7-Zip not found. Folder is ready:
    echo              %CD%\%DIST_DIR%\
    echo        Zip it manually for distribution.
)

echo.
echo ═══════════════════════════════════════════════════════════════
echo   Build + Fix + Package Complete!
echo ═══════════════════════════════════════════════════════════════
echo.
echo   Output:
echo     exe:  %EXE_PATH%
echo     dist: %CD%\%DIST_DIR%\
if defined ZIP7 echo     zip:  %CD%\%ZIP_NAME%.zip
echo.
echo   For Win7 deployment, distribute the entire
echo   %DIST_DIR%\ folder (or the .zip).
echo   Include Ghostscript + Tesseract in tools\ for OCR support.
echo.
pause