# 兴达炼铁保产事业部 - 结算单 PDF 转 Excel 工具

[![Rust](https://img.shields.io/badge/Rust-1.96.0+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-Proprietary-red.svg)]()

自动读取甲方结算单 PDF 文件，提取考核事项明细数据，生成格式化的 Excel 明细文件。

## 功能

- 📄 **PDF 解析**: 支持 pdf-extract（主） + lopdf（回退），兼容有/无文本层的 PDF
- 🔍 **OCR 支持**: 扫描件 PDF 可通过 Tesseract + Ghostscript 自动 OCR 提取文字（多页并行 + 进度条）
- 🏷️ **配置驱动分类**: 通过 YAML 文件自定义考核记录分类规则
- 📊 **Excel 生成**: 自动生成包含汇总信息、区域概览、明细数据的格式化 Excel
- 🔍 **金额闭环校验**: 自动比对 PDF 声明合计与程序提取合计，发现偏差
- 🖥️ **GUI 模式**: 无参数启动时自动进入 GUI 界面
- 📝 **文本调试**: `--dump-text` 导出 PDF 原始文本供排查

## OCR 功能（扫描件 / 图片型 PDF 支持）

当 PDF 无文本层时（扫描件、图片型 PDF），程序可通过外部工具链自动识别文字。

### 安装外部工具

#### Windows

1. **Ghostscript**  
   下载: https://ghostscript.com/releases/gsdnld.html  
   选择 `gs100xw64.exe`（64 位）或 `gs100xw32.exe`（32 位）  
   默认安装路径: `C:\Program Files\gs\gs10.x.x\bin\gswin64c.exe`

2. **Tesseract-OCR**  
   下载: https://github.com/UB-Mannheim/tesseract/wiki  
   **重要**: 安装时务必勾选 "Chinese (Simplified)" 语言包（chi_sim）  
   默认安装路径: `C:\Program Files\Tesseract-OCR\tesseract.exe`

#### Linux

```bash
sudo apt install ghostscript tesseract-ocr tesseract-ocr-chi-sim
```

### 使用方式

```bash
# 基本用法
xingda-jiesuan 扫描件.pdf --ocr

# 自定义参数
xingda-jiesuan 扫描件.pdf --ocr --ocr-dpi 600 --ocr-lang chi_sim+eng --ocr-psm 3

# 调试模式（查看 OCR 进度）
xingda-jiesuan 扫描件.pdf --ocr --log-level DEBUG
```

### OCR 参数说明

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--ocr` | 启用 OCR 通道 | 关闭 |
| `--ocr-dpi <DPI>` | 渲染分辨率（越高越清晰，但更慢） | 300 |
| `--ocr-lang <LANG>` | Tesseract 语言包（如 `chi_sim+eng`） | chi_sim |
| `--ocr-psm <PSM>` | Tesseract PSM 模式（3=全自动, 6=统一文本块） | 6 |

### 多页面并行处理

程序使用 [rayon](https://crates.io/crates/rayon) 并行处理多个页面，大幅提升 OCR 性能。  
多页 PDF 会显示实时进度条（仅在终端环境下）。

## 使用示例

```bash
# 处理单个 PDF
xingda-jiesuan 结算单.pdf

# 指定输出目录
xingda-jiesuan 结算单.pdf -o ./output/

# 批量处理目录
xingda-jiesuan -d ./pdf_folder/

# 批量处理 + 自定义命名前缀（生成 output_001.xlsx, output_002.xlsx...）
xingda-jiesuan -d ./pdf_folder/ --name output

# 自定义分类规则
xingda-jiesuan 结算单.pdf --rules custom.yaml

# 仅校验不生成 Excel
xingda-jiesuan 结算单.pdf --validate-only

# 导出 PDF 原始文本用于调试
xingda-jiesuan 结算单.pdf --dump-text

# 扫描件 OCR
xingda-jiesuan 扫描件.pdf --ocr

# 扫描件 OCR + 自定义 DPI 和语言
xingda-jiesuan 扫描件.pdf --ocr --ocr-dpi 600 --ocr-lang chi_sim+eng

# 调试模式
xingda-jiesuan 结算单.pdf --log-level DEBUG
```

## 参数

| 参数 | 说明 |
|------|------|
| `[PDF]` | 单个 PDF 文件路径 |
| `-d, --directory <DIR>` | PDF 文件目录（批量处理） |
| `-o, --output <DIR>` | 输出目录（默认当前目录） |
| `--rules <FILE>` | 自定义分类规则 YAML 文件 |
| `--validate-only` | 仅校验，不生成 Excel |
| `--dump-text` | 导出 PDF 提取的原始文本 |
| `--no-summary` | 不生成汇总信息区域 |
| `--name <NAME>` | 自定义输出文件名（批量模式为前缀） |
| `--ocr` | 启用 OCR 通道（PDF 无文本层时） |
| `--ocr-dpi <DPI>` | OCR 渲染 DPI（默认 300） |
| `--ocr-lang <LANG>` | OCR 语言包（默认 chi_sim） |
| `--ocr-psm <PSM>` | Tesseract PSM 模式 3-13（默认 6） |
| `--log-level <LEVEL>` | 日志级别：DEBUG/INFO/WARN/ERROR |

## 构建

```bash
# 依赖：Rust 1.96+ 和 Cargo

# Debug 构建
cargo build

# Release 构建（推荐）
cargo build --release

# 或使用批处理脚本
.\build_release.bat

# 可执行文件位于 target/release/xingda-jiesuan.exe
```

## 配置

### 分类规则 (classify_rules.yaml)

```yaml
department_ratio: 0.01
areas:
  - name: "事业部"
    priority: 1
    match:
      item_numbers: []
      description_patterns:
        - "协力安全管理工作方案.*落实"
        - "合同评价.*排名"

  - name: "供矿作业区"
    priority: 2
    match:
      keywords: ["供矿", "翻车", "球团"]
      equipment_prefixes: []

  - name: "煤库作业区"
    priority: 3
    match:
      keywords: ["煤库", "原煤仓", "原煤", "卸煤间"]
      equipment_prefixes: ["M"]

  - name: "原料分厂作业区"
    priority: 4
    match:
      keywords: ["原料分厂", "输入作业区", "输入区域", "原料区域", "原料输入", "原料班", "协力系统", "兴达原料作业区"]
      equipment_prefixes: ["B", "E", "F", "K", "N", "C"]

  - name: "未分类"
    priority: 99
    match: {}

area_order:
  - "事业部"
  - "原料分厂作业区"
  - "供矿作业区"
  - "煤库作业区"
  - "未分类"
```

## 项目结构

```
├── src/
│   ├── main.rs          # 入口点
│   ├── cli.rs           # 命令行接口
│   ├── parser.rs        # PDF 解析引擎
│   ├── classifier.rs    # 配置驱动分类
│   ├── excel_writer.rs  # Excel 生成
│   ├── validator.rs     # 金额闭环校验
│   ├── ocr.rs           # OCR 引擎（Tesseract + Ghostscript）
│   ├── config.rs        # 配置加载
│   ├── models.rs        # 数据模型
│   ├── error.rs         # 错误处理
│   └── gui.rs           # GUI 界面
├── classify_rules.yaml  # 默认分类规则
├── Cargo.toml           # 项目依赖
├── build.rs             # 构建脚本（嵌入配置）
└── build_release.bat    # 发布构建脚本
```

## 外部依赖 (OCR)

OCR 功能需要安装以下外部工具：

- **Ghostscript**: https://ghostscript.com/releases/gsdnld.html
- **Tesseract-OCR**: https://github.com/UB-Mannheim/tesseract/wiki
  - 安装时需勾选 `chi_sim` 中文简体语言包

## Rust 依赖

- [clap](https://crates.io/crates/clap) - 命令行参数解析
- [pdf-extract](https://crates.io/crates/pdf-extract) - PDF 文本提取
- [lopdf](https://crates.io/crates/lopdf) - PDF 回退解析
- [rust_xlsxwriter](https://crates.io/crates/rust_xlsxwriter) - Excel 文件生成
- [regex](https://crates.io/crates/regex) - 正则匹配
- [rayon](https://crates.io/crates/rayon) - 并行计算（多页面 OCR）
- [indicatif](https://crates.io/crates/indicatif) - OCR 进度条
- [eframe/egui](https://crates.io/crates/egui) - GUI 框架
- [env_logger](https://crates.io/crates/env_logger) - 日志系统
- [serde_yaml](https://crates.io/crates/serde_yaml) - YAML 配置解析

## 注意事项

- **Windows 7 运行前置条件与排障**:
  - 仅建议在 **Windows 7 SP1 x64** 上尝试运行；未安装 SP1 的 Win7 启动失败概率很高。
  - 目标机器需预先安装 **Microsoft Visual C++ 2015-2022 Redistributable (x64)**。缺少时，程序可能在双击后无响应，或直接提示 `vcruntime140.dll`、`msvcp140.dll` 缺失。
  - 即使已经用仓库内的 `fix_win7_import.py` 修复了 `GetSystemTimePreciseAsFileTime` 导入，**仍可能因为 VC++ 运行库、UCRT 或 Win7 补丁链缺失而无法启动**。
  - Win7 常见高概率原因不是业务逻辑本身，而是系统未补齐 **SHA-2 / 服务堆栈 / UCRT** 相关更新，导致 `api-ms-win-crt-*.dll`、`ucrtbase.dll` 或运行库加载失败。
  - 若提示缺少 `vcruntime140.dll`、`msvcp140.dll`：先安装/修复 **VC++ 2015-2022 x64 运行库**，重启后再试；不要随意从第三方网站单独下载 DLL 覆盖系统目录。
  - 若提示缺少 `api-ms-win-crt-*.dll`：通常表示 **UCRT 未安装完整**，需先确认 Win7 已升级到 SP1，再补齐 Windows Update 中的 UCRT / SHA-2 相关更新，然后重新安装 VC++ 运行库。
  - 可用 `dumpbin /imports xingda-jiesuan.exe` 检查导入表；若没有 `GetSystemTimePreciseAsFileTime`，说明 PE 导入修复已生效。若仍无法启动，再用 **Dependency Walker** 或新版 **Dependencies** 检查是否有缺失的 `vcruntime140.dll`、`msvcp140.dll`、`api-ms-win-crt-*.dll`。
  - `fix_win7_import.py` 的作用范围仅限于把 `kernel32.dll` 导入中的 `GetSystemTimePreciseAsFileTime` 重定向为 `GetSystemTimeAsFileTime`，**不覆盖** VC++ 运行库、UCRT、系统补丁、证书链等其它启动前置条件。
  - 最小自检清单：
    - [ ] 目标系统是 **Windows 7 SP1 x64**
    - [ ] 已安装 **Microsoft Visual C++ 2015-2022 Redistributable (x64)**
    - [ ] Windows 7 已补齐常见 **SHA-2 / 服务堆栈 / UCRT** 更新后再安装运行库
    - [ ] `dumpbin /imports` 已确认不再导入 `GetSystemTimePreciseAsFileTime`
    - [ ] 依赖检查工具未再报 `vcruntime140.dll`、`msvcp140.dll`、`api-ms-win-crt-*.dll` 缺失
- **PDF 解析**: pdf-extract 对复杂排版 PDF（带水印、多层叠加等）支持有限；PDF 无文本层时自动回退至 lopdf
- **OCR**: 扫描件/图片型 PDF 需使用 `--ocr` 标志并安装 Ghostscript 和 Tesseract；多页 PDF 会自动并行处理
- **分类精度**: 依甲方考核条款标准，特殊格式或新增条款可能需调整 `classify_rules.yaml`
- **金额校验**: 最大允许偏差 ±5%，超出则标记失败并在 Excel 中红字警告