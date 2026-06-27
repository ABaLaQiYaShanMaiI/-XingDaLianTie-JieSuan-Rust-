@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo ========================================
echo   Windows 7 兼容性修复 - 构建 & 验证
echo ========================================
echo.
echo [1/3] 清理旧构建...
cargo clean
echo.
echo [2/3] 构建 release...
cargo build --release
if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] 构建失败，请检查上方错误信息
    pause
    exit /b 1
)
echo.
echo [3/3] 验证 PE 子系统版本...
if exist "target\release\xingda-jiesuan.exe" (
    echo.
    echo ========================================
    echo   构建成功！
    echo ========================================
    echo.
    dumpbin /headers "target\release\xingda-jiesuan.exe" | findstr /i "subsystem"
    echo.
    echo 期望输出: 6.01 (Console) 或 6.01 subsystem version
    echo ========================================
) else (
    echo [ERROR] 未找到 target\release\xingda-jiesuan.exe
)
pause