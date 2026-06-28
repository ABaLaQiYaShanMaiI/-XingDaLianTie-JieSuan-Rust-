@echo off
setlocal enabledelayedexpansion

:: =====================================================================
::   Windows 7 虚拟机 - PDF 处理持续压力测试脚本
::   XingDa JieSuan v1.0.0
::
::   - 对 test_data\ 下的真实 PDF 进行全面处理测试
::   - 遇错不停，穷举所有 PDF 和参数组合
::   - 每个操作独立日志
::   - 汇总报告: test_logs\pdf_tests\_PDF_TEST_REPORT.txt
::
::   前提: 将测试用 PDF 文件放到 test_data\ 目录下
::   用法: 双击运行 或 cmd 中: win7_pdf_test.bat
:: =====================================================================

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"

:: ---- 配置 -----------------------------------------------------------
set "EXE=%SCRIPT_DIR%xingda-jiesuan.exe"
set "LOG_DIR=%SCRIPT_DIR%test_logs\pdf_tests"
set "TEST_DATA_DIR=%SCRIPT_DIR%test_data"
set "REPORT=%LOG_DIR%\_PDF_TEST_REPORT.txt"
set "TEST_COUNT=0"
set "PASS_COUNT=0"
set "FAIL_COUNT=0"
set "SKIP_COUNT=0"
set "CRASH_COUNT=0"

:: 创建日志目录
if not exist "%LOG_DIR%" mkdir "%LOG_DIR%"

echo ================================================================ > "%REPORT%"
echo   XingDa JieSuan - Win7 PDF Stress Test Report >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
echo. >> "%REPORT%"

echo ================================================================
echo   XingDa JieSuan - PDF Processing Stress Test
echo   Mode: Non-stop, exhaust all PDFs x parameters
echo ================================================================
echo.

if not exist "%EXE%" (
    echo [FATAL] xingda-jiesuan.exe not found
    echo        Place this script alongside xingda-jiesuan.exe
    pause
    exit /b 1
)

:: ---- Count PDF files -------------------------------------------------
set "PDF_FOUND=0"
for /r "%TEST_DATA_DIR%" %%f in (*.pdf) do set /a PDF_FOUND+=1

if %PDF_FOUND% equ 0 (
    echo ================================================================
    echo   [INFO] No PDF files found in test_data\
    echo.
    echo   Please copy test PDFs to: %TEST_DATA_DIR%\
    echo.
    echo   Suggested structure:
    echo     test_data\
    echo       +-- text\   (electronic PDFs with text layer^)
    echo       +-- scan\   (scanned PDFs needing OCR^)
    echo.
    echo   Place PDFs then re-run this script.
    echo ================================================================
    echo [INFO] No PDF files in test_data\ >> "%REPORT%"
    pause
    exit /b 0
)
echo   [ OK ] Found exe: %EXE%
echo   [INFO] Found %PDF_FOUND% PDF file(s^)
echo.
echo Found %PDF_FOUND% PDF file(s) >> "%REPORT%"

:: =====================================================================
:: ---- run_pdf_test helper function ----
:: call :run_pdf_test "test name" "args" "expected"
:: =====================================================================
goto :skip_run_pdf_test_func

:run_pdf_test
set /a TEST_COUNT+=1
set "TEST_NAME=%~1"
set "TEST_ARGS=%~2"
set "TEST_EXPECT=%~3"

:: Safe filename
set "SAFE_NAME=%TEST_NAME: =_%"
set "SAFE_NAME=%SAFE_NAME:/=_%"
set "SAFE_NAME=%SAFE_NAME:\=_%"
set "SAFE_NAME=%SAFE_NAME::=_%"
set "LOG_FILE=%LOG_DIR%\%TEST_COUNT%_%SAFE_NAME%.log"

echo.
echo ------------------------------------------------------------------
echo [PDF Test #%TEST_COUNT%] %TEST_NAME%
echo   Cmd: xingda-jiesuan.exe %TEST_ARGS%
echo   Expected: %TEST_EXPECT%

cmd /c ""%EXE%" %TEST_ARGS% > "%LOG_FILE%" 2>&1"
set TEST_ERR=%ERRORLEVEL%

:: Classify
set "RESULT=UNKNOWN"
if %TEST_ERR% equ 0 (
    set "RESULT=PASS"
    set /a PASS_COUNT+=1
    echo   [ PASS ] exit 0
) else if %TEST_ERR% equ 1 (
    set "RESULT=FAIL"
    set /a FAIL_COUNT+=1
    echo   [ FAIL ] exit 1 - program error
) else if %TEST_ERR% equ -1073741819 (
    set "RESULT=CRASH"
    set /a CRASH_COUNT+=1
    set /a FAIL_COUNT+=1
    echo   [ CRASH ] 0xC0000005 - ACCESS VIOLATION
) else if %TEST_ERR% equ -1073741701 (
    set "RESULT=CRASH"
    set /a CRASH_COUNT+=1
    set /a FAIL_COUNT+=1
    echo   [ CRASH ] 0xC000007B - ARCH MISMATCH
) else if %TEST_ERR% equ -1073741515 (
    set "RESULT=CRASH"
    set /a CRASH_COUNT+=1
    set /a FAIL_COUNT+=1
    echo   [ CRASH ] 0xC0000135 - DLL MISSING
) else if %TEST_ERR% lss 0 (
    set "RESULT=CRASH"
    set /a CRASH_COUNT+=1
    set /a FAIL_COUNT+=1
    echo   [ CRASH ] Negative exit: %TEST_ERR%
) else (
    set "RESULT=FAIL"
    set /a FAIL_COUNT+=1
    echo   [ FAIL ] exit %TEST_ERR%
)

:: Extract errors on failure
if /i not "!RESULT!"=="PASS" (
    echo   -- Log Snippet --
    findstr /i "error Error ERROR panic PANIC fail Fail FAIL" "%LOG_FILE%" 2>nul
    if errorlevel 1 echo     (no keyword match, see full log^)
    echo   -- Full log: %LOG_FILE%
)

:: Append to report
echo. >> "%REPORT%"
echo [PDF Test #%TEST_COUNT%] %TEST_NAME% >> "%REPORT%"
echo   Cmd: xingda-jiesuan.exe %TEST_ARGS% >> "%REPORT%"
echo   Result: !RESULT! (exit %TEST_ERR%^) >> "%REPORT%"
echo   Log: %LOG_FILE% >> "%REPORT%"
if /i not "!RESULT!"=="PASS" (
    echo   -- Errors -- >> "%REPORT%"
    findstr /i "error Error panic PANIC fail Fail" "%LOG_FILE%" >> "%REPORT%" 2>nul
)
goto :eof

:skip_run_pdf_test_func

:: =====================================================================
:: Phase 1: Basic Processing (each PDF with default params)
:: =====================================================================
echo.
echo ================================================================
echo   Phase 1: Basic PDF Processing (default params)
echo ================================================================

for /r "%TEST_DATA_DIR%" %%f in (*.pdf) do (
    set "PDF_PATH=%%f"
    set "PDF_NAME=%%~nxf"
    echo   Processing: !PDF_PATH!
    call :run_pdf_test "01-Basic-!PDF_NAME!" "!PDF_PATH!" "parse and generate Excel"
)

:: =====================================================================
:: Phase 2: Batch Directory Processing
:: =====================================================================
echo.
echo ================================================================
echo   Phase 2: Batch Directory Processing
echo ================================================================

for /d %%d in ("%TEST_DATA_DIR%\*") do (
    set "SUB_DIR=%%d"
    set "DIR_NAME=%%~nxd"
    echo   Batch dir: !SUB_DIR!
    call :run_pdf_test "02-Batch-!DIR_NAME!" "-d !SUB_DIR! -o %LOG_DIR%\batch_!DIR_NAME!" "batch process subdir"
)

:: Also batch on entire test_data
call :run_pdf_test "02-Batch-all" "-d %TEST_DATA_DIR% -o %LOG_DIR%\batch_all" "batch entire test_data"

:: =====================================================================
:: Phase 3: Parameter Combinations (each PDF x all params)
:: =====================================================================
echo.
echo ================================================================
echo   Phase 3: Parameter Combinations
echo ================================================================

for /r "%TEST_DATA_DIR%" %%f in (*.pdf) do (
    set "PDF_PATH=%%f"
    set "PDF_NAME=%%~nxf"

    call :run_pdf_test "03-Validate-!PDF_NAME!" "!PDF_PATH! --validate-only" "validate only, no Excel"
    call :run_pdf_test "03-DumpText-!PDF_NAME!" "!PDF_PATH! --dump-text -o %LOG_DIR%\dump" "export raw text"
    call :run_pdf_test "03-NoSummary-!PDF_NAME!" "!PDF_PATH! --no-summary -o %LOG_DIR%\nosum" "no summary area"
    call :run_pdf_test "03-CustomName-!PDF_NAME!" "!PDF_PATH! --name custom_report -o %LOG_DIR%\named" "custom filename"
    call :run_pdf_test "03-DebugLog-!PDF_NAME!" "!PDF_PATH! --log-level DEBUG -o %LOG_DIR%\debug" "DEBUG level log"
    call :run_pdf_test "03-SummaryOnly-!PDF_NAME!" "!PDF_PATH! --summary-only -o %LOG_DIR%\summary" "summary sheet only"
    call :run_pdf_test "03-NoMerge-!PDF_NAME!" "!PDF_PATH! --no-merge -o %LOG_DIR%\nomerge" "disable merge"
    call :run_pdf_test "03-Compact-!PDF_NAME!" "!PDF_PATH! --style compact -o %LOG_DIR%\compact" "compact style"
    call :run_pdf_test "03-Wide-!PDF_NAME!" "!PDF_PATH! --style wide -o %LOG_DIR%\wide" "wide style"
    call :run_pdf_test "03-LogFile-!PDF_NAME!" "!PDF_PATH! --log-file %LOG_DIR%\process.log -o %LOG_DIR%\logfile" "log to file"
    call :run_pdf_test "03-Combo1-!PDF_NAME!" "!PDF_PATH! --validate-only --log-level DEBUG" "validate+debug"
    call :run_pdf_test "03-Combo2-!PDF_NAME!" "!PDF_PATH! --style compact --no-summary --name combo -o %LOG_DIR%\combo" "style+name combo"
)

:: =====================================================================
:: Phase 4: OCR Tests (if tools available)
:: =====================================================================
echo.
echo ================================================================
echo   Phase 4: OCR Channel Tests
echo ================================================================

:: Check OCR tools
set "OCR_READY=0"
if exist "%SCRIPT_DIR%tools\gs\bin\gswin64c.exe" (
    if exist "%SCRIPT_DIR%tools\tesseract\tesseract.exe" set "OCR_READY=1"
)
if exist "%SCRIPT_DIR%tools\gs\bin\gswin32c.exe" (
    if exist "%SCRIPT_DIR%tools\tesseract\tesseract.exe" set "OCR_READY=1"
)

if %OCR_READY% equ 1 (
    echo   [INFO] OCR tools ready, running OCR tests

    for /r "%TEST_DATA_DIR%" %%f in (*.pdf) do (
        set "PDF_PATH=%%f"
        set "PDF_NAME=%%~nxf"

        call :run_pdf_test "04-OCR_basic-!PDF_NAME!" "!PDF_PATH! --ocr -o %LOG_DIR%\ocr" "OCR channel"
        call :run_pdf_test "04-OCR_dpi600-!PDF_NAME!" "!PDF_PATH! --ocr --ocr-dpi 600 -o %LOG_DIR%\ocr_dpi" "OCR 600 DPI"
        call :run_pdf_test "04-OCR_dpi150-!PDF_NAME!" "!PDF_PATH! --ocr --ocr-dpi 150 -o %LOG_DIR%\ocr_low" "OCR 150 DPI"
        call :run_pdf_test "04-OCR_psm3-!PDF_NAME!" "!PDF_PATH! --ocr --ocr-psm 3 -o %LOG_DIR%\ocr_psm" "OCR PSM=3"
        call :run_pdf_test "04-OCR_lang-!PDF_NAME!" "!PDF_PATH! --ocr --ocr-lang chi_sim+eng -o %LOG_DIR%\ocr_lang" "OCR dual-lang"
        call :run_pdf_test "04-OCR_combo-!PDF_NAME!" "!PDF_PATH! --ocr --ocr-dpi 400 --ocr-lang chi_sim --ocr-psm 6 --log-level DEBUG -o %LOG_DIR%\ocr_full" "OCR all params"
    )
) else (
    echo   [WARN] OCR tools (Ghostscript/Tesseract) not found - skip OCR
    echo         See tools/README.txt for setup
    set /a SKIP_COUNT+=6
    echo [SKIP] OCR tests - tools not available >> "%REPORT%"
)

:: =====================================================================
:: Phase 5: Edge Cases
:: =====================================================================
echo.
echo ================================================================
echo   Phase 5: Edge Cases
echo ================================================================

call :run_pdf_test "05-Nonexistent" "nonexistent_file.pdf" "friendly error"
call :run_pdf_test "05-Wildcard" "test_input.pdf" "handle missing file"

for /r "%TEST_DATA_DIR%" %%f in (*.pdf) do (
    set "PDF_PATH=%%f"
    set "PDF_NAME=%%~nxf"

    call :run_pdf_test "05-LowThreshold-!PDF_NAME!" "!PDF_PATH! --reward-filter-threshold 1.0 -o %LOG_DIR%\low_thresh" "threshold 1.0"
    call :run_pdf_test "05-HighScan-!PDF_NAME!" "!PDF_PATH! --reward-scan-lines 50 -o %LOG_DIR%\high_scan" "scan 50 lines"
)

:: =====================================================================
:: Phase 6: Continuous Re-run Stress
:: =====================================================================
echo.
echo ================================================================
echo   Phase 6: Continuous Re-run Stress (10 rounds)
echo ================================================================

set "STRESS_ROUNDS=10"
set "FIRST_PDF="
for /r "%TEST_DATA_DIR%" %%f in (*.pdf) do (
    if "!FIRST_PDF!"=="" set "FIRST_PDF=%%f"
)

if not "%FIRST_PDF%"=="" (
    echo   [INFO] Repeating !FIRST_PDF! %STRESS_ROUNDS% times

    set "STRESS_FAIL=0"
    set "STRESS_CRASH=0"
    for /l %%r in (1,1,%STRESS_ROUNDS%) do (
        cmd /c ""%EXE%" "!FIRST_PDF!" --validate-only >nul 2>&1"
        set ERR=!errorlevel!
        if !ERR! neq 0 (
            if !ERR! lss 0 (
                set /a STRESS_CRASH+=1
                echo   [CRASH] round %%r: exit !ERR!
            ) else (
                set /a STRESS_FAIL+=1
                echo   [FAIL] round %%r: exit !ERR!
            )
        )
    )
    echo   %STRESS_ROUNDS% rounds: %STRESS_FAIL% fails, %STRESS_CRASH% crashes
    echo Repeat %STRESS_ROUNDS%x: %STRESS_FAIL% fails, %STRESS_CRASH% crashes >> "%REPORT%"

    :: Excel generation stress
    set "STRESS_FAIL2=0"
    for /l %%r in (1,1,5) do (
        cmd /c ""%EXE%" "!FIRST_PDF!" -o "%LOG_DIR%\stress_out" --name stress_%%r >nul 2>&1"
        if !errorlevel! neq 0 (
            set /a STRESS_FAIL2+=1
            echo   [FAIL] Excel gen round %%r: exit !errorlevel!
        )
    )
    echo   5 Excel gens: %STRESS_FAIL2% fails
    echo 5x Excel gen: %STRESS_FAIL2% fails >> "%REPORT%"
) else (
    echo   [WARN] No PDF found for repeat testing
)

:: =====================================================================
:: Phase 7: Output Verification
:: =====================================================================
echo.
echo ================================================================
echo   Phase 7: Output Artifact Verification
echo ================================================================

set "XLSX_COUNT=0"
for /r "%LOG_DIR%" %%f in (*.xlsx) do set /a XLSX_COUNT+=1
echo   Excel files generated: %XLSX_COUNT%
echo Excel files: %XLSX_COUNT% >> "%REPORT%"

:: Check for temp file leakage (OCR artifacts)
set "TEMP_COUNT=0"
for /r "%LOG_DIR%" %%f in (*.png *.ppm *.pbm) do set /a TEMP_COUNT+=1
if %TEMP_COUNT% gtr 0 (
    echo   [WARN] %TEMP_COUNT% temp file(s) left over
    echo Temp files: %TEMP_COUNT% >> "%REPORT%"
) else (
    echo   [ OK ] No temp file leakage
)

:: =====================================================================
:: Summary Report
:: =====================================================================
echo.
echo ================================================================
echo   PDF STRESS TEST COMPLETE
echo ================================================================
echo.
echo   Total:   %TEST_COUNT%
echo   Passed:  %PASS_COUNT%
echo   Failed:  %FAIL_COUNT% (incl. %CRASH_COUNT% crashes^)
echo   Skipped: %SKIP_COUNT%
echo   Excel:   %XLSX_COUNT% files
echo.
echo   Logs: %LOG_DIR%\
echo   Report: %REPORT%
echo.

echo. >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
echo   PDF Test Summary >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
echo   Total:   %TEST_COUNT% >> "%REPORT%"
echo   Passed:  %PASS_COUNT% >> "%REPORT%"
echo   Failed:  %FAIL_COUNT% (%CRASH_COUNT% crashes^) >> "%REPORT%"
echo   Skipped: %SKIP_COUNT% >> "%REPORT%"
echo   Excel:   %XLSX_COUNT% files >> "%REPORT%"

:: Failure list
echo. >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
echo   Failure List >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
findstr /c:"[ FAIL" /c:"[ CRASH" "%REPORT%" >> "%REPORT%" 2>nul
if errorlevel 1 echo   (no failures) >> "%REPORT%"

:: Crash list (most critical)
echo. >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
echo   Crashes (CRITICAL) >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
findstr /c:"[ CRASH" "%REPORT%" >> "%REPORT%" 2>nul
if errorlevel 1 echo   (no crashes) >> "%REPORT%"

echo. >> "%REPORT%"
echo ================================================================ >> "%REPORT%"
echo   End of Report >> "%REPORT%"
echo ================================================================ >> "%REPORT%"

echo.
echo Press any key to exit...
pause >nul
echo [产物验证] 共生成 %XLSX_COUNT% 个 Excel 文件 >> "%REPORT%"

:: 检查是否有残存的临时文件
echo   [INFO] 检查临时文件残留...
set "TEMP_COUNT=0"
for /r "%LOG_DIR%" %%f in (*.png *.ppm *.pbm) do set /a TEMP_COUNT+=1
if %TEMP_COUNT% gtr 0 (
    echo   [WARN] 发现 %TEMP_COUNT% 个可能的 OCR 临时文件残留
    echo [产物验证] 临时文件残留: %TEMP_COUNT% 个 >> "%REPORT%"
) else (
    echo   [ OK ] 无临时文件残留
)

:: ═══════════════════════════════════════════════════════════════
:: 汇总报告
:: ═══════════════════════════════════════════════════════════════
echo.
echo ╔══════════════════════════════════════════════════════════════╗
echo ║                                                              ║
echo ║        PDF 处 理 压 力 测 试 完 成                           ║
echo ║                                                              ║
echo ╚══════════════════════════════════════════════════════════════╝
echo.
echo ═══════════════════════════════════════════════════════════════
echo   PDF 测试汇总
echo ═══════════════════════════════════════════════════════════════
echo   总测试数:   %TEST_COUNT%
echo   通过:       %PASS_COUNT%
echo   失败:       %FAIL_COUNT%  (其中 %CRASH_COUNT% 次崩溃)
echo   跳过:       %SKIP_COUNT%
echo   Excel产出:  %XLSX_COUNT% 个文件
echo ═══════════════════════════════════════════════════════════════
echo.
echo   详细日志目录: %LOG_DIR%\
echo   汇总报告:     %REPORT%
echo.

:: 写入汇总到报告
echo. >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
echo   PDF 测试汇总 >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
echo   总测试数:   %TEST_COUNT% >> "%REPORT%"
echo   通过:       %PASS_COUNT% >> "%REPORT%"
echo   失败:       %FAIL_COUNT% (其中 %CRASH_COUNT% 次崩溃) >> "%REPORT%"
echo   跳过:       %SKIP_COUNT% >> "%REPORT%"
echo   Excel产出:  %XLSX_COUNT% 个文件 >> "%REPORT%"

:: 失败列表
echo. >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
echo   失败测试列表 >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
findstr /c:"[ FAIL" /c:"[ CRASH" "%REPORT%" >> "%REPORT%" 2>nul
if %errorlevel% neq 0 echo   (无失败项) >> "%REPORT%"

:: 崩溃列表（重点关注）
echo. >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
echo   崩溃记录（需重点排查） >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
findstr /c:"[ CRASH" "%REPORT%" >> "%REPORT%" 2>nul
if %errorlevel% neq 0 echo   (无崩溃) >> "%REPORT%"

echo. >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"
echo   报告结束 >> "%REPORT%"
echo ═══════════════════════════════════════════════════════════════ >> "%REPORT%"

echo.
echo   按任意键退出...
pause >nul
exit /b 0