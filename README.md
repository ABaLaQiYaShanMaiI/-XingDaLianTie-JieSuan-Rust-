# 星达炼铁保产事业部 - 结算单 PDF 转 Excel 工具

[![Rust](https://img.shields.io/badge/Rust-1.96.0+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-Proprietary-red.svg)]()

自动读取甲方结算单 PDF 文件，提取考核事项明细数据，生成格式化的 Excel 明细文件。

## 功能

- 📄 **PDF 解析**: 支持 pdf-extract（主） + lopdf（回退），兼容有/无文本层的 PDF
- 🏷️ **配置驱动分类**: 通过 YAML 文件自定义考核记录分类规则
- 📊 **Excel 生成**: 自动生成包含汇总信息、区域概览、明细数据的格式化 Excel
- 🔍 **金额闭环校验**: 自动比对 PDF 声明合计与程序提取合计，发现偏差
- 🖥️ **GUI 模式**: 无参数启动时自动进入 GUI 界面
- 📝 **文本调试**: `--dump-text` 导出 PDF 原始文本供排查

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
department_ratio: 0.3
areas:
  - name: "事业部"
    priority: 1
    keywords: ["安全管理", "合同评价"]
    description_patterns: ["协力安全管理工作方案.*落实"]
  - name: "供矿作业区"
    priority: 2
    keywords: ["供矿", "翻车", "球团"]
    equipment_prefixes: ["G"]
  - name: "煤库作业区"
    priority: 3
    keywords: ["煤库", "原煤仓"]
    equipment_prefixes: ["M"]
  - name: "原料分厂作业区"
    priority: 4
    keywords: ["原料分厂", "输入作业区"]
    equipment_prefixes: ["B", "E", "F", "K", "N", "C"]
area_order:
  - "事业部"
  - "供矿作业区"
  - "煤库作业区"
  - "原料分厂作业区"
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
│   ├── config.rs        # 配置加载
│   ├── models.rs        # 数据模型
│   ├── error.rs         # 错误处理
│   └── gui.rs           # GUI 界面
├── classify_rules.yaml  # 默认分类规则
├── Cargo.toml           # 项目依赖
├── build.rs             # 构建脚本（嵌入配置）
└── build_release.bat    # 发布构建脚本
```

## 依赖

- [clap](https://crates.io/crates/clap) - 命令行参数解析
- [pdf-extract](https://crates.io/crates/pdf-extract) - PDF 文本提取
- [lopdf](https://crates.io/crates/lopdf) - PDF 回退解析
- [rust_xlsxwriter](https://crates.io/crates/rust_xlsxwriter) - Excel 文件生成
- [regex](https://crates.io/crates/regex) - 正则匹配
- [eframe/egui](https://crates.io/crates/egui) - GUI 框架
- [env_logger](https://crates.io/crates/env_logger) - 日志系统
- [serde_yaml](https://crates.io/crates/serde_yaml) - YAML 配置解析

## 注意事项

- **PDF 解析**: pdf-extract 对复杂排版 PDF（带水印、多层叠加等）支持有限；PDF 无文本层时自动回退至 lopdf
- **分类精度**: 依甲方考核条款标准，特殊格式或新增条款可能需调整 `classify_rules.yaml`
- **金额校验**: 最大允许偏差 ±5%，超出则标记失败并在 Excel 中红字警告