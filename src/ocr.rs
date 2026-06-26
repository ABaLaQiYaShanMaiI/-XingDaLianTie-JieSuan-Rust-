//! PDF 无文本层时通过 Ghostscript + Tesseract 实现 OCR 文字提取。
//! 管线：PDF → Ghostscript → PNG → Tesseract → 文本。
//! 需安装 Ghostscript 和 Tesseract（含 chi_sim 中文包）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use log::{info, warn, debug};
use rayon::prelude::*;
use regex::Regex;

use crate::config::ParserConfig;
use crate::error::{Result, XingDaError};

// ============================================================
// 工具路径检测
// ============================================================

/// Ghostscript 可执行文件名（按平台区分）
#[cfg(target_os = "windows")]
const GS_EXE: &str = "gswin64c.exe";
#[cfg(not(target_os = "windows"))]
const GS_EXE: &str = "gs";

/// Ghostscript 候选安装路径（Windows）
#[cfg(target_os = "windows")]
pub(crate) fn find_ghostscript() -> Option<PathBuf> {
    // 1. 检查环境变量
    if let Ok(path) = std::env::var("GHOSTSCRIPT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    // 2. EXE 同目录 tools/ 便携版（优先）
    if let Some(tools_path) = tools_dir_gs() {
        if tools_path.exists() {
            return Some(tools_path);
        }
    }

    // 3. 直接尝试 PATH 中查找
    if let Some(found) = which_cmd(GS_EXE) {
        return Some(found);
    }

    // 4. 扫描常见安装目录
    let candidate_dirs = [
        r"C:\Program Files\gs",
        r"C:\Program Files (x86)\gs",
    ];

    for base in &candidate_dirs {
        let base_path = Path::new(base);
        if base_path.exists() {
            if let Ok(entries) = fs::read_dir(base_path) {
                for entry in entries.flatten() {
                    let dir = entry.path();
                    if dir.is_dir() {
                        let gs_path = dir.join("bin").join(GS_EXE);
                        if gs_path.exists() {
                            return Some(gs_path);
                        }
                    }
                }
            }
        }
    }

    None
}

/// 检查 EXE 同级 tools/ 目录下的 Ghostscript 便携版
#[cfg(target_os = "windows")]
fn tools_dir_gs() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("tools").join(GS_EXE),
        exe_dir.join(GS_EXE),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(target_os = "macos")]
pub(crate) fn find_ghostscript() -> Option<PathBuf> {
    // 1. 检查环境变量
    if let Ok(path) = std::env::var("GHOSTSCRIPT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    // 2. 尝试 which gs
    if let Some(found) = which_cmd(GS_EXE) {
        return Some(found);
    }
    // 3. Homebrew 标准路径 (含 M1/M2 ARM)
    let candidates = [
        "/usr/local/bin/gs",
        "/opt/local/bin/gs",               // MacPorts
        "/usr/local/opt/ghostscript/bin/gs",
        "/opt/homebrew/bin/gs",            // Apple Silicon Homebrew
    ];
    for p in &candidates {
        if Path::new(p).exists() {
            return Some(PathBuf::from(p));
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn find_ghostscript() -> Option<PathBuf> {
    // 1. 检查环境变量
    if let Ok(path) = std::env::var("GHOSTSCRIPT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    which_cmd(GS_EXE)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) fn find_ghostscript() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("GHOSTSCRIPT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    which_cmd(GS_EXE)
}

/// Tesseract 可执行文件名
#[cfg(target_os = "windows")]
const TESSERACT_EXE: &str = "tesseract.exe";
#[cfg(not(target_os = "windows"))]
const TESSERACT_EXE: &str = "tesseract";

/// Tesseract 候选安装路径（Windows）
#[cfg(target_os = "windows")]
pub(crate) fn find_tesseract() -> Option<PathBuf> {
    // 1. 检查环境变量
    if let Ok(path) = std::env::var("TESSERACT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    // 2. EXE 同目录 tools/ 便携版（优先）
    if let Some(tools_path) = tools_dir_tesseract() {
        if tools_path.exists() {
            return Some(tools_path);
        }
    }

    // 3. 直接尝试 PATH 中查找
    if let Some(found) = which_cmd(TESSERACT_EXE) {
        return Some(found);
    }

    // 4. 扫描常见安装目录
    let candidates = [
        r"C:\Program Files\Tesseract-OCR\tesseract.exe",
        r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
    ];

    for path in &candidates {
        if Path::new(path).exists() {
            return Some(PathBuf::from(path));
        }
    }

    None
}

/// 检查 EXE 同级 tools/ 目录下的 Tesseract 便携版
#[cfg(target_os = "windows")]
fn tools_dir_tesseract() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("tools").join(TESSERACT_EXE),
        exe_dir.join(TESSERACT_EXE),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(target_os = "macos")]
pub(crate) fn find_tesseract() -> Option<PathBuf> {
    // 1. 检查环境变量
    if let Ok(path) = std::env::var("TESSERACT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    // 2. 尝试 which
    if let Some(found) = which_cmd(TESSERACT_EXE) {
        return Some(found);
    }
    // 3. Homebrew 标准路径
    let candidates = [
        "/usr/local/bin/tesseract",
        "/opt/local/bin/tesseract",
        "/opt/homebrew/bin/tesseract",
    ];
    for p in &candidates {
        if Path::new(p).exists() {
            return Some(PathBuf::from(p));
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn find_tesseract() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("TESSERACT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    which_cmd(TESSERACT_EXE)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) fn find_tesseract() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("TESSERACT_PATH") {
        let p = Path::new(&path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    which_cmd(TESSERACT_EXE)
}

/// 检查命令是否在 PATH 中可用（跨平台）
///
/// Windows: 使用 `where` 命令
/// Linux/macOS: 使用 `which` 命令
#[cfg(target_os = "windows")]
fn which_cmd(cmd: &str) -> Option<PathBuf> {
    let output = Command::new("where")
        .arg(cmd)
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // `where` 可能返回多条路径，取第一条
        stdout.lines().next().map(PathBuf::from)
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn which_cmd(cmd: &str) -> Option<PathBuf> {
    let output = Command::new("which")
        .arg(cmd)
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next().map(|s| PathBuf::from(s.trim()))
    } else {
        None
    }
}

// ============================================================
// OCR 主管线
// ============================================================

/// OCR 处理结果
#[derive(Debug)]
pub struct OcrResult {
    /// 提取的文本内容
    pub text: String,
    /// 处理的页数
    pub page_count: usize,
}

/// 对 PDF 执行 OCR 提取文本
///
/// # 参数
/// - `pdf_path`: PDF 文件路径
/// - `config`: 解析器配置（OCR引擎、DPI、语言、PSM 模式等）
///
/// # 流程
/// 1. 使用 Ghostscript 将 PDF 每页转为 PNG
/// 2. 对每张 PNG 并行调用 Tesseract 识别文字
/// 3. 合并所有页面文本
pub fn perform_ocr(pdf_path: &str, config: &ParserConfig) -> Result<OcrResult> {
    // 根据配置的 OCR 引擎分发
    match config.ocr_engine {
        crate::config::OcrEngine::Tesseract => perform_ocr_tesseract(pdf_path, config),
    }
}

/// 使用 Tesseract OCR 引擎提取文本
fn perform_ocr_tesseract(pdf_path: &str, config: &ParserConfig) -> Result<OcrResult> {
    let ocr_start = Instant::now();

    let gs_path = find_ghostscript()
        .ok_or_else(|| XingDaError::Pdf(
            "未找到 Ghostscript。请安装 Ghostscript 后重试。\n\
             下载: https://ghostscript.com/releases/gsdnld.html".to_string()
        ))?;

    let tesseract_path = find_tesseract()
        .ok_or_else(|| XingDaError::Pdf(
            "未找到 Tesseract-OCR。请安装 Tesseract 后重试。\n\
             下载: https://github.com/UB-Mannheim/tesseract/wiki\n\
             （安装时请勾选中文简体语言包 chi_sim）".to_string()
        ))?;

    info!("OCR 管线初始化:");
    info!("  引擎:        Tesseract");
    info!("  Ghostscript: {}", gs_path.display());
    info!("  Tesseract:   {}", tesseract_path.display());
    info!("  DPI:         {}", config.ocr_dpi);
    info!("  语言包:      {}", config.ocr_lang);
    info!("  PSM 模式:    {}", config.tesseract_psm);

    let pdf = Path::new(pdf_path);
    let pdf_stem = pdf.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ocr_temp");

    // 临时目录（使用唯一名称避免路径冲突/编码问题）
    let temp_dir = {
        let base = std::env::temp_dir();
        let unique_name = format!("xingda_ocr_{}", std::process::id());
        base.join(&unique_name)
    };
    fs::create_dir_all(&temp_dir)
        .map_err(|e| XingDaError::Pdf(format!("无法创建 OCR 临时目录: {}", e)))?;

    // 清理函数（最佳实践）
    let cleanup = || {
        if let Err(e) = fs::remove_dir_all(&temp_dir) {
            debug!("清理 OCR 临时文件失败: {}", e);
        }
    };

    // 确保退出时清理
    let _cleanup_guard = scopeguard::guard((), |_| cleanup());

    // ---- 步骤 1: PDF → PNG (Ghostscript) ----
    let dpi = config.ocr_dpi;
    let output_pattern = temp_dir.join(format!("{}_page_%04d.png", pdf_stem));
    let output_pattern_str = output_pattern.to_str()
        .ok_or_else(|| XingDaError::Pdf("OCR 临时路径包含非法字符".to_string()))?;

    info!("  步骤 1/2: PDF → PNG ({} DPI)...", dpi);

    // Ghostscript 命令: gs -dNOPAUSE -dBATCH -sDEVICE=png16m -r300
    //   -sOutputFile=pattern -q input.pdf
    let gs_cmd = Command::new(&gs_path)
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-dSAFER",
            "-sDEVICE=png16m",
            &format!("-r{}", dpi),
        ])
        .args([
            &format!("-sOutputFile={}", output_pattern_str),
            "-q",
            pdf_path,
        ])
        .output()
        .map_err(|e| XingDaError::Pdf(format!("Ghostscript 执行失败: {}\n请确保 Ghostscript 已正确安装", e)))?;

    if !gs_cmd.status.success() {
        let stderr = String::from_utf8_lossy(&gs_cmd.stderr);
        cleanup();
        return Err(XingDaError::Pdf(format!(
            "Ghostscript 渲染 PDF 页面失败:\n{}", stderr
        )));
    }
    info!("    Ghostscript 渲染完成");

    // ---- 收集生成的 PNG 文件 ----
    let mut png_files: Vec<PathBuf> = Vec::new();
    // 按编号收集
    for i in 1..10000 {
        // 预分配时用的 %04d 模式，按 1-based 编号
        let png_path = temp_dir.join(format!("{}_page_{:04}.png", pdf_stem, i));
        if png_path.exists() {
            png_files.push(png_path);
        } else {
            break;
        }
    }

    if png_files.is_empty() {
        cleanup();
        return Err(XingDaError::Pdf("Ghostscript 未生成任何页面图像".to_string()));
    }

    let page_count = png_files.len();
    info!("    生成 {} 个页面图像", page_count);

    // ---- 步骤 2: PNG → 文本 (Tesseract) 并行处理 ----
    info!("  步骤 2/2: Tesseract OCR ({})...", config.ocr_lang);

    let tesseract_start = Instant::now();

    // 使用进度条
    let ocr_results: Vec<(usize, Result<String>)> = match try_init_progress_bar(page_count) {
        Some(bar) => {
            // 带进度条的并行处理
            let results: Vec<(usize, Result<String>)> = png_files
                .par_iter()
                .enumerate()
                .map(|(i, png_path)| {
                    let page_num = i + 1;
                    let result = ocr_single_page(
                        png_path,
                        &tesseract_path,
                        config,
                    );
                    bar.inc(1);
                    (page_num, result)
                })
                .collect();
            bar.finish_with_message("OCR 完成");
            results
        }
        None => {
            // 无进度条回退（并行）
            let results: Vec<(usize, Result<String>)> = png_files
                .par_iter()
                .enumerate()
                .map(|(i, png_path)| {
                    let page_num = i + 1;
                    debug!("    OCR 第 {} 页...", page_num);
                    let result = ocr_single_page(
                        png_path,
                        &tesseract_path,
                        config,
                    );
                    (page_num, result)
                })
                .collect();
            results
        }
    };

    let tesseract_elapsed = tesseract_start.elapsed();
    info!("    Tesseract 耗时: {:.2}s", tesseract_elapsed.as_secs_f64());

    // 合并结果（按页号排序）
    let mut sorted = ocr_results;
    sorted.sort_by_key(|(page_num, _)| *page_num);

    let mut all_text = String::new();
    for (page_num, result) in &sorted {
        match result {
            Ok(text) => {
                if !text.trim().is_empty() {
                    if !all_text.is_empty() {
                        all_text.push('\n');
                    }
                    all_text.push_str(text);
                }
                debug!("    第 {} 页: {} 字符", page_num, text.len());
            }
            Err(e) => {
                warn!("    第 {} 页 OCR 失败: {}", page_num, e);
            }
        }
    }

    let ocr_total_elapsed = ocr_start.elapsed();
    info!("    OCR 完成，共提取 {} 字符，总耗时: {:.2}s",
        all_text.len(), ocr_total_elapsed.as_secs_f64());

    if all_text.trim().is_empty() {
        cleanup();
        return Err(XingDaError::Pdf(
            "OCR 未能从 PDF 中提取任何文本。\n\
             可能原因:\n\
             - PDF 为空白页或纯图片且文字模糊\n\
             - Tesseract 中文语言包 (chi_sim) 未安装\n\
             - 图片分辨率过低".to_string()
        ));
    }

    Ok(OcrResult {
        text: all_text,
        page_count,
    })
}

/// 对单页 PNG 执行 Tesseract OCR
fn ocr_single_page(
    png_path: &Path,
    tesseract_path: &Path,
    config: &ParserConfig,
) -> Result<String> {
    let txt_base = png_path.with_extension(""); // Tesseract 自动加 .txt
    let txt_base_str = txt_base.to_str()
        .ok_or_else(|| XingDaError::Pdf("OCR 输出路径包含非法字符".to_string()))?;

    let tesseract_cmd = Command::new(tesseract_path)
        .args([
            png_path.to_str().unwrap(),
            txt_base_str,
            "-l", &config.ocr_lang,
            "--psm", &config.tesseract_psm.to_string(),
        ])
        .output()
        .map_err(|e| XingDaError::Pdf(format!(
            "Tesseract 执行失败: {}\n请确保 Tesseract-OCR 已正确安装并包含中文语言包 ({})",
            e, config.ocr_lang
        )))?;

    if !tesseract_cmd.status.success() {
        let stderr = String::from_utf8_lossy(&tesseract_cmd.stderr);
        // 警告而非错误——Tesseract 可能成功生成了输出文本但有 stderr 信息
        warn!("    Tesseract stderr: {}", stderr.trim());
    }

    // Tesseract 输出文件名为 txt_base.txt
    let txt_path = PathBuf::from(format!("{}.txt", txt_base_str));
    if txt_path.exists() {
        let raw = fs::read_to_string(&txt_path)
            .map_err(|e| XingDaError::Pdf(format!("读取 OCR 输出失败: {}", e)))?;
        let cleaned = clean_ocr_text(&raw);
        Ok(cleaned)
    } else {
        Err(XingDaError::Pdf(format!(
            "Tesseract 未生成输出文件: {}",
            txt_path.display()
        )))
    }
}

/// 初始化进度条。indicatif 在非 TTY 环境下自动隐藏输出。
fn try_init_progress_bar(page_count: usize) -> Option<indicatif::ProgressBar> {
    // 如果页面只有 1 页，进度条意义不大
    if page_count <= 1 {
        return None;
    }

    let bar = indicatif::ProgressBar::new(page_count as u64);
    bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} 页 ({eta})")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ ")
    );
    Some(bar)
}

// ============================================================
// OCR 文本清洗
// ============================================================

/// 清洗 OCR 产出的原始文本
///
/// 主要处理：
/// - 合并被 OCR 拆分的短行
/// - 移除空白页产生的孤立字符
/// - 统一全角/半角混用
fn clean_ocr_text(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::new();

    // 合并短行（OCR 常把一个句子拆成多个短行）
    let mut pending = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !pending.is_empty() {
                result.push(pending.clone());
                pending.clear();
            }
            continue;
        }

        // 如果当前行以序号或日期开头，先提交 pending
        if is_new_record_start(trimmed) && !pending.is_empty() {
            result.push(pending.clone());
            pending.clear();
        }

        if pending.is_empty() {
            pending = trimmed.to_string();
        } else {
            // 合并到当前行（去重空格）
            pending.push(' ');
            pending.push_str(trimmed);
        }
    }
    if !pending.is_empty() {
        result.push(pending);
    }

    // 将常见全角数字和标点转为半角
    let full_to_half: String = result.join("\n");
    let half_nums = full_to_half
        .replace('\u{ff10}', "0")
        .replace('\u{ff11}', "1")
        .replace('\u{ff12}', "2")
        .replace('\u{ff13}', "3")
        .replace('\u{ff14}', "4")
        .replace('\u{ff15}', "5")
        .replace('\u{ff16}', "6")
        .replace('\u{ff17}', "7")
        .replace('\u{ff18}', "8")
        .replace('\u{ff19}', "9")
        .replace('\u{ff0e}', ".")
        .replace('\u{ff0c}', ",");

    half_nums
}

/// 判断行是否为新考核条目的起始（以序号+日期开头）
///
/// 示例：`"1  3月 11日，原料分厂..."`、`"12  2025年 合同评价..."`、`"5  近 3 年 安全记录..."`
fn is_new_record_start(line: &str) -> bool {
    let re = Regex::new(
        r"^\s*\d{1,2}\s+(\d+\s*-\s*\d+\s*月|\d+\s*月|近\s*\d+\s*年|\d{4}\s*年|[一二三四五六七八九十]+\s*季度)"
    ).unwrap();
    re.is_match(line)
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── 工具路径检测 ───────────────────────────────────

    #[test]
    fn test_which_cmd() {
        // Windows: cmd.exe 应始终存在于 PATH 中
        // Linux/macOS: sh 应始终存在
        #[cfg(target_os = "windows")]
        let found = which_cmd("cmd.exe");
        #[cfg(not(target_os = "windows"))]
        let found = which_cmd("sh");

        assert!(found.is_some(), "which_cmd should find a system shell");
        let path = found.unwrap();
        assert!(path.to_str().is_some(), "path should be valid UTF-8");
    }

    #[test]
    fn test_which_cmd_nonexistent() {
        let found = which_cmd("this_command_does_not_exist_xyzzy");
        assert!(found.is_none(), "which_cmd should return None for nonexistent command");
    }

    #[test]
    fn test_find_ghostscript_fallback() {
        // Ghostscript 可能未安装，函数不应 panic
        // 在 CI 或开发环境中，这通常返回 None
        let result = find_ghostscript();
        // 函数必须能正常执行完毕（不会 panic）
        if let Some(ref path) = result {
            // 如果找到了，应该是一个有效路径
            assert!(path.to_str().is_some());
        }
    }

    #[test]
    fn test_find_tesseract_fallback() {
        let result = find_tesseract();
        if let Some(ref path) = result {
            assert!(path.to_str().is_some());
        }
    }

    // ── OCR 管线错误处理 ───────────────────────────────

    #[test]
    fn test_ocr_pipeline_without_tools() {
        // 使用不存在的 PDF 路径，应返回错误而非 panic
        let config = ParserConfig::default();
        // 注意: perform_ocr 需要找到 Ghostscript/Tesseract，
        // 在没有安装这些工具的测试环境中应该返回明确的错误信息
        let result = perform_ocr("nonexistent_file.pdf", &config);

        match result {
            Err(XingDaError::Pdf(msg)) => {
                // 错误消息应该包含有用的安装指导
                assert!(
                    msg.contains("Ghostscript") || msg.contains("Tesseract"),
                    "Error should mention missing tool: {}",
                    msg
                );
            }
            Ok(_) => {
                // 如果工具恰好安装了，也算通过
            }
            Err(_other) => {
                // 其他类型错误也可能（比如 PDF 不存在先被检测）
                // 这也是有效的——只要不 panic
            }
        }
    }

    #[test]
    fn test_perform_ocr_missing_pdf() {
        // 如果工具存在但 PDF 不存在，应报告有意义的错误
        let gs = find_ghostscript();
        let ts = find_tesseract();
        if gs.is_some() && ts.is_some() {
            let config = ParserConfig::default();
            let result = perform_ocr("this_pdf_definitely_does_not_exist.pdf", &config);
            assert!(result.is_err(), "perform_ocr should fail with missing PDF");
        }
    }

    // ── 文本清洗 ─────────────────────────────────────

    #[test]
    fn test_is_new_record_start() {
        assert!(is_new_record_start("1  3月 11日，原料分厂"));
        assert!(is_new_record_start("12  2025年 合同评价"));
        assert!(is_new_record_start("5  近 3 年 安全记录"));
        assert!(!is_new_record_start("普通描述文字"));
        assert!(!is_new_record_start(""));
    }

    #[test]
    fn test_clean_ocr_text_full_width_numbers() {
        let input = "考核金额 \u{ff11}\u{ff10}\u{ff10}\u{ff0e}\u{ff10}\u{ff10} 元\n合计 \u{ff12}\u{ff10}\u{ff10}\u{ff10}.00";
        let result = clean_ocr_text(input);
        // 全角数字和点号已转换为半角
        assert!(result.contains("100.00"));
        assert!(result.contains("2000.00"));
    }

    #[test]
    fn test_clean_ocr_text_merge_lines() {
        let input = "1  3月 11日，\n原料分厂违反规定\n100.00\n\n2  3月 12日，\n检查不合格\n200.00";
        let result = clean_ocr_text(input);
        // 验证合并行数和基本结构，避免编码差异导致的精确字符串匹配失败
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2, "should merge into 2 lines, got:\n{}", result);
        assert!(lines[0].starts_with("1 "), "line 0 should start with '1 ', got: {}", lines[0]);
        assert!(lines[0].ends_with("100.00"), "line 0 should end with '100.00', got: {}", lines[0]);
        assert!(lines[1].starts_with("2 "), "line 1 should start with '2 ', got: {}", lines[1]);
        assert!(lines[1].ends_with("200.00"), "line 1 should end with '200.00', got: {}", lines[1]);
    }

    // ── 集成测试（需手动启用）─────────────────────────────

    #[test]
    #[ignore] // 需要外部工具 (Ghostscript + Tesseract)
    fn test_ocr_end_to_end() {
        // 此测试需要一个扫描件 PDF 样本文件
        // 将样本 PDF 放在项目根目录的 test_data/ 下
        let sample_pdf = "test_data/scan_sample.pdf";
        if !Path::new(sample_pdf).exists() {
            println!("跳过集成测试：缺少样本 PDF {}", sample_pdf);
            return;
        }

        let config = ParserConfig::default();
        let result = perform_ocr(sample_pdf, &config);

        match result {
            Ok(ocr_result) => {
                assert!(ocr_result.page_count > 0, "应至少处理 1 页");
                assert!(!ocr_result.text.trim().is_empty(), "OCR 应提取到文本");

                // 验证输出文本包含期望关键词（根据实际样本调整）
                println!("OCR 页数: {}", ocr_result.page_count);
                println!("OCR 文本长度: {}", ocr_result.text.len());
                println!("--- OCR 输出预览 ---");
                let preview: String = ocr_result.text.lines().take(20).collect::<Vec<_>>().join("\n");
                println!("{}", preview);
            }
            Err(e) => {
                // 如果工具未安装，跳过而非失败
                if e.to_string().contains("未找到") {
                    println!("跳过集成测试：OCR 工具未安装");
                } else {
                    panic!("OCR 端到端测试失败: {}", e);
                }
            }
        }
    }

    #[test]
    #[ignore] // 需要外部工具
    fn test_ocr_with_custom_config() {
        let sample_pdf = "test_data/scan_sample.pdf";
        if !Path::new(sample_pdf).exists() {
            return;
        }

        // 使用自定义配置
        let config = ParserConfig {
            ocr_dpi: 150, // 低 DPI 更快
            ocr_lang: "chi_sim".to_string(),
            tesseract_psm: 3, // 全自动页面分割
            ..Default::default()
        };

        let result = perform_ocr(sample_pdf, &config);
        assert!(result.is_ok() || result.unwrap_err().to_string().contains("未找到"));
    }
}