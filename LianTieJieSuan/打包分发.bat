@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ═══════════════════════════════════════════════════════════════
echo   兴达结算工具 - 分发打包脚本
echo ═══════════════════════════════════════════════════════════════
echo.

:: ============================================================
:: 配置
:: ============================================================
set "PROJECT_ROOT=%~dp0.."
set "DIST_DIR=%~dp0"
set "RELEASE_EXE=%PROJECT_ROOT%\target\release\xingda-jiesuan.exe"
set "RULES_FILE=%PROJECT_ROOT%\classify_rules.yaml"
set "PACKAGE_NAME=LianTieJieSuan"

echo [1/4] 检查 Release 可执行文件...
if not exist "%RELEASE_EXE%" (
    echo [错误] 未找到 Release 构建产物: %RELEASE_EXE%
    echo        请先执行: cargo build --release
    echo        或运行: build_release.bat
    pause
    exit /b 1
)
echo       已找到: %RELEASE_EXE%

echo.
echo [2/4] 复制可执行文件到 LianTieJieSuan/ ...
copy /Y "%RELEASE_EXE%" "%DIST_DIR%\" >nul
if %errorlevel% neq 0 (
    echo [错误] 复制失败
    pause
    exit /b 1
)
echo       已复制: xingda-jiesuan.exe

echo.
echo [3/4] 复制分类规则文件到 LianTieJieSuan/ ...
copy /Y "%RULES_FILE%" "%DIST_DIR%\" >nul
if %errorlevel% neq 0 (
    echo [警告] classify_rules.yaml 复制失败，exe 内置规则仍可用
)
echo       已复制: classify_rules.yaml

echo.
echo [4/4] 创建压缩包...

:: 获取 7-Zip 路径（如果安装了的话）
set "ZIP7="
if exist "C:\Program Files\7-Zip\7z.exe" set "ZIP7=C:\Program Files\7-Zip\7z.exe"
if exist "C:\Program Files (x86)\7-Zip\7z.exe" set "ZIP7=C:\Program Files (x86)\7-Zip\7z.exe"

if defined ZIP7 (
    "%ZIP7%" a -tzip "%DIST_DIR%..\%PACKAGE_NAME%.zip" "%DIST_DIR%*" -xr!".gitkeep" >nul
    if %errorlevel% equ 0 (
        echo       已生成: %DIST_DIR%..\%PACKAGE_NAME%.zip
    ) else (
        echo [警告] 7-Zip 打包失败
    )
) else (
    echo [提示] 未找到 7-Zip，跳过压缩包创建
    echo        可手动将 LianTieJieSuan\ 文件夹打包成 .zip 分发
)

echo.
echo ═══════════════════════════════════════════════════════════════
echo   打包完成！
echo ═══════════════════════════════════════════════════════════════
echo.
echo   LianTieJieSuan\ 目录内容:
dir /B "%DIST_DIR%"
echo.
echo   提醒:
echo   - tools\ 目录需手动放入 Ghostscript 和 Tesseract 便携版
echo     详见 LianTieJieSuan\tools\README.txt
echo   - 将 LianTieJieSuan\ 文件夹直接打包成 .zip 即可分发
echo.
pause