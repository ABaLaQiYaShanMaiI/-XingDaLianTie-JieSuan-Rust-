//! 命令行参数解析与单文件/批量处理入口。

use std::path::{Path, PathBuf};

use clap::{Parser, ValueHint};
use log::{info, error, LevelFilter};

use crate::config::{load_rules, load_excel_style, ParserConfig, StylePreset, ClassifyRules, ExcelStyle};
use crate::models::SettlementData;
use crate::parser::parse_pdf;
use crate::classifier::classify_records;
use crate::validator::{validate_amounts, generate_validation_summary};
use crate::excel_writer::generate_excel;
use crate::error::{Result, XingDaError};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 兴达炼铁保产事业部 - PDF结算单转Excel明细工具
#[derive(Parser, Debug)]
#[command(
    name = "xingda-jiesuan",
    version = VERSION,
    about = "自动读取甲方结算单PDF文件，提取考核事项明细数据，生成格式化的Excel明细文件",
    long_about = None,
    after_help = "使用示例:\n  xingda-jiesuan 结算单.pdf\n  xingda-jiesuan 结算单.pdf -o ./output/\n  xingda-jiesuan -d ./pdf_folder/\n  xingda-jiesuan 结算单.pdf --rules custom.yaml\n  xingda-jiesuan 结算单.pdf --validate-only\n  xingda-jiesuan 结算单.pdf --log-level DEBUG"
)]
pub struct Cli {
    /// PDF 文件路径
    #[arg(value_hint = ValueHint::FilePath)]
    pub pdf: Option<String>,

    /// PDF 文件目录（批量处理）
    #[arg(short = 'd', long = "directory", value_hint = ValueHint::DirPath)]
    pub directory: Option<String>,

    /// 输出目录（默认桌面）
    #[arg(short = 'o', long = "output", value_hint = ValueHint::DirPath)]
    pub output: Option<String>,

    /// 分类规则配置文件路径（YAML 格式）
    #[arg(long = "rules", value_hint = ValueHint::FilePath)]
    pub rules: Option<String>,

    /// 仅解析和校验，不生成 Excel 文件
    #[arg(long = "validate-only", default_value_t = false)]
    pub validate_only: bool,

    /// 日志输出级别
    #[arg(long = "log-level", default_value = "INFO")]
    pub log_level: String,

    /// 导出 PDF 提取的原始文本为 .txt 文件
    #[arg(long = "dump-text", default_value_t = false)]
    pub dump_text: bool,

    /// 不生成汇总信息区域
    #[arg(long = "no-summary", default_value_t = false)]
    pub no_summary: bool,

    /// 自定义输出文件名（单文件模式为精确文件名，批量模式作为前缀_001/002...）
    #[arg(long = "name")]
    pub name: Option<String>,

    /// 启用 OCR 模式（PDF 无文本层时自动调用 Tesseract + Ghostscript）
    #[arg(long = "ocr", default_value_t = false)]
    pub ocr: bool,

    /// OCR DPI（默认 300）
    #[arg(long = "ocr-dpi", default_value = "300")]
    pub ocr_dpi: u32,

    /// OCR 语言（默认 chi_sim）
    #[arg(long = "ocr-lang", default_value = "chi_sim")]
    pub ocr_lang: String,

    /// Tesseract PSM 模式 3-13（默认 6）
    #[arg(long = "ocr-psm", default_value = "6")]
    pub ocr_psm: u8,

    /// 日志文件输出路径（超过 5MB 时自动归档为 .old，仅保留一个旧文件）
    #[arg(long = "log-file", value_hint = ValueHint::FilePath)]
    pub log_file: Option<String>,

    /// Excel 样式预设（compact: 紧凑, wide: 宽松）
    #[arg(long = "style", value_enum)]
    pub style: Option<StylePreset>,

    /// 仅生成汇总 sheet，跳过区域明细
    #[arg(long = "summary-only", default_value_t = false)]
    pub summary_only: bool,

    /// 禁用多行合并（调试用）
    #[arg(long = "no-merge", default_value_t = false)]
    pub no_merge: bool,

    /// 嘉奖金额扫描行数（当嘉奖金额距"嘉奖金额"行较远时增大此值，默认 5）
    #[arg(long = "reward-scan-lines", default_value = "5")]
    pub reward_scan_lines: usize,

    /// 嘉奖金额最小过滤阈值（元，低于此值视为噪音，需调小时可降低，默认 10.0）
    #[arg(long = "reward-filter-threshold", default_value = "10.0")]
    pub reward_filter_threshold: f64,
}

/// 设置日志系统（支持控制台 + 可选日志文件）
pub fn setup_logging(level: &str, log_file: Option<&str>) {
    let level = match level.to_uppercase().as_str() {
        "DEBUG" => LevelFilter::Debug,
        "INFO" => LevelFilter::Info,
        "WARNING" | "WARN" => LevelFilter::Warn,
        "ERROR" => LevelFilter::Error,
        _ => LevelFilter::Info,
    };

    let mut builder = env_logger::Builder::new();
    builder.filter_level(level).format_timestamp_secs();

    // 如果指定了日志文件，追加文件输出
    if let Some(path) = log_file {
        let ts = chrono_timestamp();
        let log_path = format!("{}_{}.log", path.trim_end_matches(".log"), ts);

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                let file_writer =
                    std::sync::Mutex::new(std::io::BufWriter::new(file));
                builder.target(env_logger::Target::Pipe(Box::new(LogFileWriter {
                    file: file_writer,
                    path: log_path.clone(),
                    max_size: 5 * 1024 * 1024, // 5MB
                })));
                eprintln!("日志文件: {}", log_path);
            }
            Err(e) => {
                eprintln!("无法创建日志文件 {}: {}", log_path, e);
            }
        }
    }

    builder.init();
}

/// 生成日志文件时间戳字符串（跨平台安全格式）
/// 使用 chrono crate 替代 time crate：chrono 在 Windows 上仅使用
/// GetSystemTimeAsFileTime (Win7 兼容)，不会调用 Win8+ 才有的
/// GetSystemTimePreciseAsFileTime，避免 Win7 上启动报错。
fn chrono_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

/// 日志文件写入器（超过 5MB 自动归档为 .old，每次 write 检查文件元数据）
// TODO: 轮转逻辑在每次 write 中检查元数据，高频日志可能影响性能；
//       可考虑在打开文件时记录初始长度并计数，或使用 tracing-appender 等轮转库。
struct LogFileWriter {
    file: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
    path: String,
    max_size: u64,
}

impl std::io::Write for LogFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut f = self.file.lock().unwrap();
        if let Ok(meta) = f.get_ref().metadata() {
            if meta.len() > self.max_size {
                let backup = format!("{}.old", self.path);
                let _ = std::fs::rename(&self.path, &backup);
                let new_file = std::fs::File::create(&self.path)?;
                *f = std::io::BufWriter::new(new_file);
            }
        }
        f.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.lock().unwrap().flush()
    }
}

/// PDF 处理选项（将 process_single 的 16 个参数提炼为结构体）
pub struct PdfProcessOptions<'a> {
    pub output_dir: Option<&'a str>,
    pub rules_path: Option<&'a str>,
    pub validate_only: bool,
    pub dump_text: bool,
    pub include_summary: bool,
    pub summary_only: bool,
    pub output_name: Option<&'a str>,
    pub enable_ocr: bool,
    pub no_merge: bool,
    pub parser_config: &'a ParserConfig,
    pub style: Option<StylePreset>,
}

/// 核心处理流程：解析 + 分类 + 校验，返回 SettlementData 和配置引用，
/// 供 CLI 和 GUI 复用。
///
/// 返回 `(SettlementData, ClassifyRules, ExcelStyle, bool)`。
/// `bool` 为 `is_valid` 校验结果。
pub fn process_pdf_core(
    pdf_path: &str,
    rules_path: Option<&str>,
    enable_ocr: bool,
    no_merge: bool,
    parser_config: &ParserConfig,
    style: Option<StylePreset>,
) -> Result<(SettlementData, ClassifyRules, ExcelStyle, bool)> {
    let rules = load_rules(rules_path)?;
    let mut excel_style = load_excel_style();
    if let Some(style_preset) = style {
        excel_style = excel_style.apply_preset(style_preset);
    }

    let mut data = parse_pdf(pdf_path, enable_ocr, no_merge, parser_config)?;

    classify_records(&mut data, &rules);
    let is_valid = validate_amounts(&mut data);

    Ok((data, rules, excel_style, is_valid))
}

/// 处理单个 PDF 文件（委托给 process_single_with_options）
pub fn process_single(
    pdf_path: &str,
    output_dir: Option<&str>,
    rules_path: Option<&str>,
    validate_only: bool,
    dump_text: bool,
    include_summary: bool,
    summary_only: bool,
    output_name: Option<&str>,
    enable_ocr: bool,
    no_merge: bool,
    parser_config: &ParserConfig,
    style: Option<StylePreset>,
) -> Result<String> {
    let options = PdfProcessOptions {
        output_dir,
        rules_path,
        validate_only,
        dump_text,
        include_summary,
        summary_only,
        output_name,
        enable_ocr,
        no_merge,
        parser_config,
        style,
    };
    process_single_with_options(pdf_path, &options)
}

/// 使用 PdfProcessOptions 处理单个 PDF 文件（推荐新调用方使用）
pub fn process_single_with_options(
    pdf_path: &str,
    options: &PdfProcessOptions,
) -> Result<String> {
    let pdf_p = Path::new(pdf_path);
    if !pdf_p.exists() {
        return Err(XingDaError::Parse(format!("PDF 文件不存在: {}", pdf_path)));
    }

    let out_dir = PathBuf::from(options.output_dir.unwrap_or("."));
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| XingDaError::Parse(format!("无法创建输出目录: {}", e)))?;

    // --- 核心处理：解析 + 分类 + 校验 ---
    let (data, rules, excel_style, is_valid) = process_pdf_core(
        pdf_path,
        options.rules_path,
        options.enable_ocr,
        options.no_merge,
        options.parser_config,
        options.style,
    )?;

    let summary = generate_validation_summary(&data);
    info!("\n{}", summary);

    // 导出原始文本
    if options.dump_text {
        let txt_path = pdf_p.with_extension("txt");
        let txt_filename = txt_path
            .file_name()
            .ok_or_else(|| XingDaError::Parse("PDF文件名无法解析".into()))?;
        let txt_out = out_dir.join(txt_filename);
        let header = format!(
            "=== PDF 原始文本导出 ===\n文件: {}\n提取字符数: {}\n=========================\n\n",
            pdf_path,
            data.raw_text.len()
        );
        let content = header + &data.raw_text;
        std::fs::write(&txt_out, &content)
            .map_err(|e| XingDaError::Parse(format!("写入文本文件失败 {}: {}", txt_out.display(), e)))?;
        info!("原始文本已导出: {} ({} 字符)", txt_out.display(), data.raw_text.len());
    }

    if options.validate_only {
        if is_valid {
            info!("--validate-only 模式：解析和校验完成，未生成 Excel");
        } else {
            error!("金额闭环校验失败，请检查解析结果");
        }
        return Ok(String::new());
    }

    // --- 生成 Excel ---
    let excel_name = if let Some(name) = options.output_name {
        let mut n = name.to_string();
        if !n.ends_with(".xlsx") {
            n.push_str(".xlsx");
        }
        n
    } else {
        let stem = pdf_p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        format!("{}明细.xlsx", stem)
    };

    let output_path = out_dir.join(&excel_name);
    let output_path_str = output_path
        .to_str()
        .ok_or_else(|| XingDaError::ExcelWrite("输出路径包含非法字符".to_string()))?;

    generate_excel(
        &data,
        output_path_str,
        &rules.area_order,
        &excel_style,
        options.include_summary,
        options.summary_only,
    )?;

    let records_count = data.all_records.len();
    let total = data.total_assessment;
    let status = if is_valid { "✅ 校验通过" } else { "❌ 校验失败" };
    info!(
        "解析 {} 条考核记录 / 总金额 ¥{:.2} / {}",
        records_count, total, status
    );

    Ok(output_path_str.to_string())
}

/// 批量处理目录下的所有 PDF 文件
pub fn batch_process(
    input_dir: &str,
    output_dir: Option<&str>,
    rules_path: Option<&str>,
    validate_only: bool,
    dump_text: bool,
    include_summary: bool,
    summary_only: bool,
    output_name: Option<&str>,
    enable_ocr: bool,
    no_merge: bool,
    parser_config: &ParserConfig,
    style: Option<StylePreset>,
) -> Result<Vec<String>> {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    let dir = Path::new(input_dir);
    let mut pdf_files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "pdf").unwrap_or(false) {
                pdf_files.push(path);
            }
        }
    }

    pdf_files.sort();
    let total = pdf_files.len();
    info!("发现 {} 个 PDF 文件", total);

    let index_width = if total > 0 {
        ((total as f64).log10().floor() as usize) + 1
    } else {
        1
    };

    for (i, pdf_file) in pdf_files.iter().enumerate() {
        let pdf_path = pdf_file.to_str().unwrap_or("");

        let batch_name: Option<String> = if let Some(base_name) = output_name {
            let stem = base_name.trim_end_matches(".xlsx");
            Some(format!(
                "{}_{:0width$}.xlsx",
                stem,
                i + 1,
                width = index_width
            ))
        } else {
            None
        };

        match process_single(
            pdf_path,
            output_dir,
            rules_path,
            validate_only,
            dump_text,
            include_summary,
            summary_only,
            batch_name.as_deref(),
            enable_ocr,
            no_merge,
            parser_config,
            style,
        ) {
            Ok(result) => {
                if result.is_empty() {
                    info!(
                        "  ✅ {} 校验完成（未生成文件）",
                        pdf_file
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or("")
                    );
                } else {
                    let filename = Path::new(&result)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    results.push(result);
                    info!(
                        "  ✅ {} → {}",
                        pdf_file
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or(""),
                        filename
                    );
                }
            }
            Err(e) => {
                let err_msg = format!(
                    "  ❌ {}: {}",
                    pdf_file
                        .file_name()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or(""),
                    e
                );
                errors.push(err_msg.clone());
                error!("{}", err_msg);
            }
        }
    }

    if !results.is_empty() || !errors.is_empty() {
        info!("\n{}", "═".repeat(50));
        info!("批量处理完成:");
        info!("  成功: {} 个", results.len());
        info!("  失败: {} 个", errors.len());
        if !errors.is_empty() {
            info!("  失败详情:");
            for err in &errors {
                info!("    {}", err);
            }
        }
    }

    Ok(results)
}

/// CLI 主入口
pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    setup_logging(&cli.log_level, cli.log_file.as_deref());

    let include_summary = !cli.no_summary;

    let parser_config = ParserConfig {
        ocr_dpi: cli.ocr_dpi,
        ocr_lang: cli.ocr_lang.clone(),
        tesseract_psm: cli.ocr_psm,
        reward_scan_lines: cli.reward_scan_lines,
        reward_filter_threshold: cli.reward_filter_threshold,
        ..Default::default()
    };

    let output_dir = cli.output.or_else(|| {
        dirs::desktop_dir().map(|p| p.to_string_lossy().to_string())
    });

    if let Some(ref dir) = cli.directory {
        batch_process(
            dir,
            output_dir.as_deref(),
            cli.rules.as_deref(),
            cli.validate_only,
            cli.dump_text,
            include_summary,
            cli.summary_only,
            cli.name.as_deref(),
            cli.ocr,
            cli.no_merge,
            &parser_config,
            cli.style,
        )?;
    } else if let Some(ref pdf) = cli.pdf {
        let output = process_single(
            pdf,
            output_dir.as_deref(),
            cli.rules.as_deref(),
            cli.validate_only,
            cli.dump_text,
            include_summary,
            cli.summary_only,
            cli.name.as_deref(),
            cli.ocr,
            cli.no_merge,
            &parser_config,
            cli.style,
        )?;
        if !output.is_empty() {
            info!("\n输出文件: {}", output);
        }
    } else {
        #[cfg(feature = "gui")]
        {
            crate::gui::launch_gui();
            return Ok(());
        }
        #[cfg(not(feature = "gui"))]
        {
            Cli::parse_from(&["xingda-jiesuan", "--help"]);
        }
    }

    Ok(())
}