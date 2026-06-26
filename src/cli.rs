//! 命令行接口模块
//! =============
//! 提供增强的命令行参数支持。

use std::path::{Path, PathBuf};

use clap::{Parser, ValueHint};
use log::{info, error, LevelFilter};

use crate::config::{load_rules, load_excel_style};
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

    /// 输出目录
    #[arg(short = 'o', long = "output", value_hint = ValueHint::DirPath, default_value = ".")]
    pub output: String,

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
}

/// 设置日志系统
pub fn setup_logging(level: &str) {
    let level = match level.to_uppercase().as_str() {
        "DEBUG" => LevelFilter::Debug,
        "INFO" => LevelFilter::Info,
        "WARNING" | "WARN" => LevelFilter::Warn,
        "ERROR" => LevelFilter::Error,
        _ => LevelFilter::Info,
    };

    env_logger::Builder::new()
        .filter_level(level)
        .format_timestamp_secs()
        .init();
}

/// 处理单个 PDF 文件
pub fn process_single(
    pdf_path: &str,
    output_dir: Option<&str>,
    rules_path: Option<&str>,
    validate_only: bool,
    dump_text: bool,
    include_summary: bool,
    output_name: Option<&str>,
) -> Result<String> {
    let pdf_p = Path::new(pdf_path);
    if !pdf_p.exists() {
        return Err(XingDaError::Parse(format!("PDF 文件不存在: {}", pdf_path)));
    }

    // 加载配置
    let rules = load_rules(rules_path)?;
    let excel_style = load_excel_style();

    // 创建输出目录（提前，供 dump-text 和 Excel 共用）
    let out_dir = PathBuf::from(output_dir.unwrap_or("."));
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| XingDaError::Parse(format!("无法创建输出目录: {}", e)))?;

    // --- 1. 解析 PDF ---
    let mut data = parse_pdf(pdf_path)?;

    // 导出原始文本（调试用）
    if dump_text {
        let txt_filename = pdf_p.with_extension("txt").file_name()
            .ok_or_else(|| XingDaError::Parse("PDF文件名无法解析".into()))?;
        let txt_out = out_dir.join(txt_filename);
        std::fs::write(&txt_out, &data.raw_text)
            .map_err(|e| XingDaError::Parse(format!("写入文本文件失败 {}: {}", txt_out.display(), e)))?;
        info!("原始文本已导出: {}", txt_out.display());
    }

    // --- 2. 分类 ---
    classify_records(&mut data, &rules);

    // --- 3. 校验 ---
    let is_valid = validate_amounts(&mut data);
    let summary = generate_validation_summary(&data);
    info!("\n{}", summary);

    if validate_only {
        if is_valid {
            info!("--validate-only 模式：解析和校验完成，未生成 Excel");
        } else {
            error!("金额闭环校验失败，请检查解析结果");
        }
        return Ok(String::new());
    }

    // --- 4. 生成 Excel ---
    let excel_name = if let Some(name) = output_name {
        let mut n = name.to_string();
        if !n.ends_with(".xlsx") {
            n.push_str(".xlsx");
        }
        n
    } else {
        let stem = pdf_p.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        format!("{}明细.xlsx", stem)
    };

    let output_path = out_dir.join(&excel_name);
    let output_path_str = output_path.to_str()
        .ok_or_else(|| XingDaError::ExcelWrite("输出路径包含非法字符".to_string()))?;

    generate_excel(
        &data,
        output_path_str,
        &rules.area_order,
        &excel_style,
        include_summary,
    )?;

    // 打印汇总
    let records_count = data.all_records.len();
    let total = data.total_assessment;
    let status = if is_valid { "✅ 校验通过" } else { "❌ 校验失败" };
    info!(
        "解析 {} 条考核记录 / 总金额 ¥{:,.2} / {}",
        records_count, total, status
    );

    Ok(output_path_str.to_string())
}

/// 批量处理目录下的所有 PDF 文件
/// --name 在批量模式下作为前缀使用，每个文件生成 `name_001.xlsx` `name_002.xlsx` ... 避免覆盖
pub fn batch_process(
    input_dir: &str,
    output_dir: Option<&str>,
    rules_path: Option<&str>,
    validate_only: bool,
    dump_text: bool,
    include_summary: bool,
    output_name: Option<&str>,
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

    // 计算序号宽度（用于零填充）
    let index_width = if total > 0 { ((total as f64).log10().floor() as usize) + 1 } else { 1 };

    for (i, pdf_file) in pdf_files.iter().enumerate() {
        let pdf_path = pdf_file.to_str().unwrap_or("");

        // 批量模式下，如果指定了 --name，作为前缀加序号
        let batch_name: Option<String> = if let Some(base_name) = output_name {
            let stem = base_name.trim_end_matches(".xlsx");
            Some(format!("{}_{:0width$}.xlsx", stem, i + 1, width = index_width))
        } else {
            None // 使用默认命名（各 PDF 文件名+明细）
        };

        match process_single(
            pdf_path,
            output_dir,
            rules_path,
            validate_only,
            dump_text,
            include_summary,
            batch_name.as_deref(),
        ) {
            Ok(result) => {
                if result.is_empty() {
                    info!("  ✅ {} 校验完成（未生成文件）",
                        pdf_file.file_name().unwrap_or_default().to_str().unwrap_or(""));
                } else {
                    let filename = Path::new(&result).file_name().and_then(|s| s.to_str()).unwrap_or("");
                    results.push(result);
                    info!("  ✅ {} → {}",
                        pdf_file.file_name().unwrap_or_default().to_str().unwrap_or(""), filename);
                }
            }
            Err(e) => {
                let err_msg = format!("  ❌ {}: {}",
                    pdf_file.file_name().unwrap_or_default().to_str().unwrap_or(""), e);
                errors.push(err_msg.clone());
                error!("{}", err_msg);
            }
        }
    }

    // 批量汇总
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
    setup_logging(&cli.log_level);

    let include_summary = !cli.no_summary;

    if let Some(ref dir) = cli.directory {
        batch_process(
            dir,
            Some(&cli.output),
            cli.rules.as_deref(),
            cli.validate_only,
            cli.dump_text,
            include_summary,
            cli.name.as_deref(),
        )?;
    } else if let Some(ref pdf) = cli.pdf {
        let output = process_single(
            pdf,
            Some(&cli.output),
            cli.rules.as_deref(),
            cli.validate_only,
            cli.dump_text,
            include_summary,
            cli.name.as_deref(),
        )?;
        if !output.is_empty() {
            info!("\n输出文件: {}", output);
        }
    } else {
        // 无参数时启动 GUI
        #[cfg(feature = "gui")]
        {
            crate::gui::launch_gui();
            return Ok(());
        }
        #[cfg(not(feature = "gui"))]
        {
            // 打印 help
            Cli::parse_from(&["xingda-jiesuan", "--help"]);
        }
    }

    Ok(())
}