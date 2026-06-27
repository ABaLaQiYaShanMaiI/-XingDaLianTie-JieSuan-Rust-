chcp 65001 >nul
cd /d "%~dp0"
echo ========================================
echo   Windows 7 兼容性修复 - 构建 ^& 验证
echo ========================================
echo.
echo [1/4] 清理旧构建...
cargo clean
echo.
echo [2/4] 构建 release...
cargo build --release
if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] 构建失败，请检查上方错误信息
    pause
    exit /b 1
)
echo.
echo [3/4] 修复 PE 导入表 (Win7 兼容)...
if exist "target\release\xingda-jiesuan.exe" (
    python fix_win7_import.py "target\release\xingda-jiesuan.exe"
    if %ERRORLEVEL% neq 0 (
        echo [WARN] PE 导入表修复失败，但可执行文件可能仍然可用
        echo        若在 Windows 7 上运行报错，请手动运行:
        echo        python fix_win7_import.py target\release\xingda-jiesuan.exe
    )
) else (
    echo [ERROR] 未找到 target\release\xingda-jiesuan.exe
    pause
    exit /b 1
)
echo.
echo [4/4] 验证...
if exist "target\release\xingda-jiesuan.exe" (
    echo.
    echo ========================================
    echo   构建成功！
    echo ========================================
    echo.
    echo --- PE 子系统版本 ---
    dumpbin /headers "target\release\xingda-jiesuan.exe" | findstr /i "subsystem"
    echo.
    echo --- kernel32 导入 ---
    dumpbin /imports "target\release\xingda-jiesuan.exe" | findstr /i Get
