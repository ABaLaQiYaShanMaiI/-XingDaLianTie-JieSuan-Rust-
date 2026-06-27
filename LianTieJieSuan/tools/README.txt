═══════════════════════════════════════════════════════════════
  外置工具说明
  兴达炼铁结算单 PDF 转 Excel 工具
═══════════════════════════════════════════════════════════════

本目录用于存放程序运行所需的外部工具。
程序启动时会自动检查此目录，优先级高于系统 PATH。

═══════════════════════════════════════════════════════════════
  需要放置的文件
═══════════════════════════════════════════════════════════════

1. Ghostscript（必需 - OCR 功能）
   文件名: gswin64c.exe
   下载地址: https://ghostscript.com/releases/gsdnld.html
   
   便携版获取方式:
   a. 下载官方安装包 gs100xw64.exe 并安装
   b. 从安装目录（如 C:\Program Files\gs\gs10.x.x\bin\）复制以下文件:
      - gswin64c.exe  → tools/
      - gsdll64.dll   → tools/
      - 其余 bin/ 下的 .dll 文件 → tools/

2. Tesseract-OCR（必需 - OCR 功能）
   文件名: tesseract.exe
   下载地址: https://github.com/UB-Mannheim/tesseract/wiki
   
   便携版获取方式:
   a. 下载 tesseract-ocr-w64-setup-5.x.x.exe 并安装
   b. 从安装目录（如 C:\Program Files\Tesseract-OCR\）复制:
      - tesseract.exe  → tools/
      - 所有 .dll 文件  → tools/
      - tessdata/ 文件夹 → tools/tessdata/
        （必须包含 chi_sim.traineddata 中文简体语言包）

═══════════════════════════════════════════════════════════════
  便携版目录结构示例
═══════════════════════════════════════════════════════════════

LianTieJieSuan/
├── xingda-jiesuan.exe
├── classify_rules.yaml
└── tools/
    ├── gswin64c.exe
    ├── gsdll64.dll
    ├── tesseract.exe
    ├── leptonica-util.dll
    ├── libpng16.dll
    ├── libtiff-6.dll
    ├── libjpeg-62.dll
    ├── libgif-7.dll
    ├── libwebp-7.dll
    ├── libopenjp2-7.dll
    ├── libleptonica.dll
    ├── libtesseract.dll
    └── tessdata/
        ├── chi_sim.traineddata
        ├── eng.traineddata
        └── ...

═══════════════════════════════════════════════════════════════
  注意事项
═══════════════════════════════════════════════════════════════

- 如果目标机器已安装 Ghostscript / Tesseract 到系统，
  则 tools/ 目录可为空，程序会自动从系统 PATH 查找。

- 如果 tools/ 目录存在工具，则优先使用 tools/ 中的版本。

- classify_rules.yaml 已内嵌在 exe 中，外部文件仅作覆盖用。