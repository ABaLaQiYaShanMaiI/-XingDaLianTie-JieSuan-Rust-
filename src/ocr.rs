//! OCR 模块（扫描型 / 图片型 PDF 支持）
//! =====================================
//! 当 PDF 无文本层时，通过外部工具链实现 OCR 提取文字：
//!
//! 工具链流程：
//!   PDF 页面 → Ghostscript (gs) → PNG 图像 → Tesseract OCR → 文本
//!
//! # 外部依赖
//!
//! ## Windows
//! - **Ghostscript**: 下载 gs100xw64.exe 或 gs100xw32.exe
//!   https://ghostscript.com/releases/gsdnld.html
//!   安装后默认路径: `C:\Program Files\gs\gs10.0x.x\bin\gswin64c.exe`
//! - **Tesseract**: 下载安装包
//!   https://github.com/UB-Mannheim/tesseract/wiki
//!   安装时勾选中文简体语言包 (chi_sim)
//!   默认路径: `C:\Program Files\Tesseract-OCR\tesseract.exe`
//!
//! ## Linux
//! ```bash
//! sudo apt install ghostscript tesseract-ocr tesseract-ocr-chi-sim
//! ```
//!
//! # 使用方式
//! ```bash
//! xingda-jiesuan 扫描件.pdf --ocr
//! ```
//!
//! 程序自动检测 PDF 是否有文本层：
//! - 有文本层 → 直接用 pdf-extract / lopdf 解析
//! - 无文本层 + --ocr 标志 → 启动 OCR 管线
//! - 无文本层 + 无 --ocr 标志 → 报错退出

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use log::{info, warn, debug};
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
fn find_ghostscript() -> Option<PathBuf> {
    // 1. 直接尝试 PATH 中查找
    if which_cmd(GS_EXE).is_some() {
        return Some(PathBuf::from(GS_EXE));
    }

    // 2. 扫描常见安装目录
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
                        for sub in ["bin", "bin"] {
                            let gs_path = dir.join(sub).join(GS_EXE);
                            if gs_path.exists() {
                                return Some(gs_path);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn find_ghostscript() -> Option<PathBuf> {
    // Linux/macOS: rely on PATH
    which_cmd(GS_EXE)
}

/// Tesseract 可执行文件名
#[cfg(target_os = "windows")]
const TESSERACT_EXE: &str = "tesseract.exe";
#[cfg(not(target_os = "windows"))]
const TESSERACT_EXE: &str = "tesseract";

/// Tesseract 候选安装路径（Windows）
#[cfg(target_os = "windows")]
fn find_tesseract() -> Option<PathBuf> {
    if which_cmd(TESSERACT_EXE).is_some() {
        return Some(PathBuf::from(TESSERACT_EXE));
    }

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

#[cfg(not(target_os = "windows"))]
fn find_tesseract() -> Option<PathBuf> {
    which_cmd(TESSERACT_EXE)
}

/// 检查命令是否在 PATH 中可用
fn which_cmd(cmd: &str) -> Option<PathBuf> {
    let output = Command::new("where")
        .arg(cmd)
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next().map(PathBuf::from)
    } else {
        None
    }
}

// ============================================================
// OCR 主管线
// ============================================================

/// OCR 处理结果
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
/// - `config`: 解析器配置（对 OCR 无直接作用，预留）
///
/// # 流程
/// 1. 使用 Ghostscript 将 PDF 每页转为 PNG
/// 2. 对每张 PNG 调用 Tesseract 识别文字
/// 3. 合并所有页面文本
pub fn perform_ocr(pdf_path: &str, config: &ParserConfig) -> Result<OcrResult> {
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
    info!("  Ghostscript: {}", gs_path.display());
    info!("  Tesseract:   {}", tesseract_path.display());

    let pdf = Path::new(pdf_path);
    let pdf_stem = pdf.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ocr_temp");

    // 临时目录
    let temp_dir = std::env::temp_dir().join("xingda_ocr");
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

    // ---- 步骤 2: PNG → 文本 (Tesseract) ----
    info!("  步骤 2/2: Tesseract OCR (chi_sim)...");
    let mut all_text = String::new();

    for (i, png_path) in png_files.iter().enumerate() {
        let page_num = i + 1;
        let txt_base = png_path.with_extension(""); // Tesseract 自动加 .txt
        let txt_base_str = txt_base.to_str()
            .ok_or_else(|| XingDaError::Pdf("OCR 输出路径包含非法字符".to_string()))?;

        debug!("    OCR 第 {} 页: {}", page_num, png_path.display());

        let tesseract_cmd = Command::new(&tesseract_path)
            .args([
                png_path.to_str().unwrap(),
                txt_base_str,
                "-l", "chi_sim",        // 中文简体语言
                "--psm", "6",           // 统一文本块模式（对混合中英文排版友好）
            ])
            .output()
            .map_err(|e| XingDaError::Pdf(format!(
                "Tesseract 执行失败: {}\n请确保 Tesseract-OCR 已正确安装并包含中文语言包 (chi_sim)", e
            )))?;

        if !tesseract_cmd.status.success() {
            let stderr = String::from_utf8_lossy(&tesseract_cmd.stderr);
            warn!("    第 {} 页 OCR 警告: {}", page_num, stderr.trim());
        }

        // Tesseract 输出文件名为 txt_base.txt
        let txt_path = PathBuf::from(format!("{}.txt", txt_base_str));
        if txt_path.exists() {
            match fs::read_to_string(&txt_path) {
                Ok(text) => {
                    let cleaned = clean_ocr_text(&text);
                    if !cleaned.trim().is_empty() {
                        all_text.push_str(&cleaned);
                        all_text.push('\n');
                    }
                    debug!("    第 {} 页: {} 字符", page_num, cleaned.len());
                }
                Err(e) => {
                    warn!("    第 {} 页 OCR 结果读取失败: {}", page_num, e);
                }
            }
        } else {
            warn!("    第 {} 页 OCR 未生成输出文件", page_num);
        }
    }

    info!("    OCR 完成，共提取 {} 字符", all_text.len());

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

    // 全角数字 → 半角
    let full_to_half: String = result.join("\n");
    let half_nums = full_to_half
        .replace('０', "0")
        .replace('１', "1")
        .replace('２', "2")
        .replace('３', "3")
        .replace('４', "4")
        .replace('５', "5")
        .replace('６', "6")
        .replace('７', "7")
        .replace('８', "8")
        .replace('９', "9")
        .replace('．', ".")
        .replace('，', ",");

    half_nums
}

/// 判断行是否为新考核条目的起始（以序号+日期开头）
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
        let input = "考核金额 １００．００ 元\n合计 ２０００.00";
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
}