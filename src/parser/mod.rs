//! 使用 pdf-extract / lopdf 提取 PDF 文本，通过正则匹配解析考核记录。

mod fee;
mod reward;

use std::path::Path;
use std::fs;

use log::{info, warn, debug};
use regex::Regex;

use crate::error::{Result, XingDaError};
use crate::models::{AssessmentRecord, SettlementData};
use crate::config::ParserConfig;

// ============================================================
// 加载正则
// ============================================================

/// 考核记录描述前缀（共用，兼容空格情况）
///
/// 匹配示例：`"3月 11日，..."`、`"2025年 ..."`、`"近 3 年 ..."`、`"一季度 ..."`
fn assessment_desc_re() -> &'static Regex {
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"^(?:\d+\s*-\s*\d+\s*月|\d+\s*月|近\s*\d+\s*年|\d{4}\s*年|[一二三四五六七八九十]+\s*季度)").unwrap()
    });
    &RE
}

/// 考核记录行模式：序号 + 日期/年份描述开头
fn record_line_re() -> &'static Regex {
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"^(\d{1,2})\s+(\d+\s*-\s*\d+\s*月|\d+\s*月|近\s*\d+\s*年|\d{4}\s*年|[一二三四五六七八九十]+\s*季度)").unwrap()
    });
    &RE
}

/// 多条款模式
fn clause_patterns() -> &'static [Regex] {
    static RE: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
        vec![
            Regex::new(r"炼铁厂.*?(?:条款|办法).*?(?:\d+\.\d+)?").unwrap(),
            Regex::new(r"协力供应商.*?(?:标准|条款).*?(?:\d+\.\d+)?").unwrap(),
            Regex::new(r"检修协力.*?(?:标准|条款).*?(?:\d+\.\d+)?").unwrap(),
            Regex::new(r"协力供应商安全违约记分抵扣标准.*?(?:\d+\.\d+)?").unwrap(),
            Regex::new(r"炼铁厂生产协力供应商绩效评价条款.*?(?:\d+\.\d+)?").unwrap(),
        ]
    });
    &RE
}

/// 非考核行过滤模式
fn non_assessment_patterns() -> &'static [Regex] {
    static RE: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
        vec![
            Regex::new(r"自查隐患").unwrap(),
            Regex::new(r"整改闭环").unwrap(),
            Regex::new(r"嘉奖金额").unwrap(),
            Regex::new(r"作业费用").unwrap(),
            Regex::new(r"合同考核").unwrap(),
            Regex::new(r"考核金额合计").unwrap(),
            Regex::new(r"当月结算费用").unwrap(),
            Regex::new(r"小计").unwrap(),
            Regex::new(r"乙方考核").unwrap(),
            Regex::new(r"安全考核").unwrap(),
        ]
    });
    &RE
}

/// 边界检测模式（考核小计 / 嘉奖标题）
fn boundary_patterns() -> &'static [Regex] {
    static RE: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
        vec![
            Regex::new(r"^小计\s*\d+").unwrap(),
            Regex::new(r"^合同嘉奖").unwrap(),
            Regex::new(r"^嘉奖金额").unwrap(),
        ]
    });
    &RE
}

// ============================================================
// 主解析入口
// ============================================================

/// 解析 PDF 结算单，返回 SettlementData 对象
///
/// `enable_ocr`: 当 PDF 无文本层时，是否启用 Tesseract+Ghostscript OCR
/// `no_merge`: 禁用多行合并（调试用）
/// `parser_config`: 解析器/Parser配置（OCR DPI、语言等）
pub fn parse_pdf(pdf_path: &str, enable_ocr: bool, no_merge: bool, parser_config: &ParserConfig) -> Result<SettlementData> {
    info!("正在读取 PDF: {}", pdf_path);

    let path = Path::new(pdf_path);
    if !path.exists() {
        return Err(XingDaError::Parse(format!("PDF 文件不存在: {}", pdf_path)));
    }

    let mut data = SettlementData::new();
    data.pdf_path = Some(pdf_path.to_string());

    // --- 提取 PDF 文本 ---
    let (full_text, used_ocr) = extract_pdf_text(pdf_path, enable_ocr, parser_config)?;
    data.from_ocr = used_ocr;

    if full_text.trim().is_empty() {
        return Err(XingDaError::Parse(format!("PDF 无文本内容: {}", pdf_path)));
    }

    // 保存原始文本（用于 --dump-text）
    data.raw_text = full_text.clone();

    // --- 提取合同基本信息 ---
    extract_contract_info(&mut data, &full_text);

    // --- 提取费用信息 ---
    fee::extract_fee_info(&mut data, &full_text, parser_config);

    // --- 文本行提取考核记录 ---
    let records = extract_from_text(&full_text, no_merge, parser_config);

    info!("文本通道提取 {} 条记录", records.len());

    data.all_records = records;

    Ok(data)
}

/// 使用 pdf-extract 提取 PDF 文本，无文本层时可选 OCR 回退
fn extract_pdf_text(pdf_path: &str, enable_ocr: bool, parser_config: &ParserConfig) -> Result<(String, bool)> {
    let bytes = fs::read(pdf_path)
        .map_err(|e| XingDaError::Parse(format!("无法读取 PDF 文件: {}", e)))?;

    // 使用 pdf-extract 提取文本
    match pdf_extract::extract_text(pdf_path) {
        Ok(text) => {
            if text.trim().is_empty() {
                // 尝试用 lopdf 解析原始文本
                let doc = lopdf::Document::load_mem(&bytes)
                    .map_err(|e| XingDaError::Parse(format!("PDF 解析失败: {}", e)))?;
                let mut all_text = String::new();
                for (page_num, _) in doc.page_iter().enumerate() {
                    if let Ok(text) = doc.extract_text(&[(page_num as u32 + 1)]) {
                        all_text.push_str(&text);
                        all_text.push('\n');
                    }
                }
                if !all_text.trim().is_empty() {
                    return Ok((all_text, false));
                }

                // pdf-extract 空 + lopdf 空 → 无文本层
                if enable_ocr {
                    info!("PDF 无文本层，启用 OCR 通道（需 Ghostscript + Tesseract）");
                    let ocr_result = crate::ocr::perform_ocr(pdf_path, parser_config)?;
                    info!(
                        "OCR 完成: {} 页，共 {} 字符",
                        ocr_result.page_count,
                        ocr_result.text.len()
                    );
                    return Ok((ocr_result.text, true));
                } else {
                    return Err(XingDaError::Pdf(
                        "PDF 无文本层。请使用 --ocr 标志启用 OCR 通道（需安装 Ghostscript 和 Tesseract）。\n\
                         或参考 README 中的安装说明。".to_string()
                    ));
                }
            }
            Ok((text, false))
        }
        Err(e) => {
            // 回退到 lopdf
            warn!("pdf-extract 失败: {}, 尝试 lopdf 回退", e);
            let doc = lopdf::Document::load_mem(&bytes)
                .map_err(|e2| XingDaError::Parse(format!("PDF 解析失败: pdf-extract={}, lopdf={}", e, e2)))?;
            let mut all_text = String::new();
            for page_num in 1..=doc.get_pages().len() as u32 {
                match doc.extract_text(&[page_num]) {
                    Ok(text) => {
                        all_text.push_str(&text);
                        all_text.push('\n');
                    }
                    Err(_) => {
                        debug!("第 {} 页文本提取失败", page_num);
                    }
                }
            }
            if all_text.trim().is_empty() {
                if enable_ocr {
                    info!("pdf-extract 和 lopdf 均无法提取文本，启用 OCR 通道");
                    let ocr_result = crate::ocr::perform_ocr(pdf_path, parser_config)?;
                    info!(
                        "OCR 完成: {} 页，共 {} 字符",
                        ocr_result.page_count,
                        ocr_result.text.len()
                    );
                    return Ok((ocr_result.text, true));
                } else {
                    return Err(XingDaError::Pdf(
                        "PDF 无文本层且 pdf-extract / lopdf 均无法解析。\n\
                         请使用 --ocr 标志启用 OCR 通道（需安装 Ghostscript 和 Tesseract）。\n\
                         或参考 README 中的安装说明。".to_string()
                    ));
                }
            }
            Ok((all_text, false))
        }
    }
}

// ============================================================
// 合同信息提取
// ============================================================

fn extract_first(text: &str, pattern: &Regex, group: usize) -> String {
    pattern
        .captures(text)
        .and_then(|caps| caps.get(group))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default()
}

/// 提取合同基本信息
fn extract_contract_info(data: &mut SettlementData, full_text: &str) {
    let contract_no_re = Regex::new(r"合同编号[：:]\s*((?:SC|HT|W)-\S+)").unwrap();
    let work_period_re = Regex::new(r"作业时间[：:]\s*([^\n]+)").unwrap();
    let contract_name_re = Regex::new(r"合同名称[：:]\s*([^\n]+)").unwrap();

    data.contract_no = extract_first(full_text, &contract_no_re, 1);
    data.work_period = extract_first(full_text, &work_period_re, 1);
    data.contract_name = extract_first(full_text, &contract_name_re, 1);

    // 提取月份标签
    let month_re = Regex::new(r"(\d{4})\s*年\s*(\d{1,2})\s*月").unwrap();
    if let Some(caps) = month_re.captures(&data.work_period) {
        if let Some(m) = caps.get(2) {
            data.month_label = format!("{}月", m.as_str().parse::<i32>().unwrap_or(0));
        }
    } else {
        // 从文件名提取
        if let Some(pdf_path) = &data.pdf_path {
            let fname = Path::new(pdf_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let ym_re = Regex::new(r"(\d{4})(\d{2})").unwrap();
            if let Some(caps) = ym_re.captures(fname) {
                if let Some(m) = caps.get(2) {
                    data.month_label = format!("{}月", m.as_str().parse::<i32>().unwrap_or(0));
                }
            }
        }
    }

    info!("  合同编号: {}", data.contract_no);
    info!("  合同名称: {}", data.contract_name);
    info!("  作业时间: {}", data.work_period);
}

// ============================================================
// 文本行提取考核记录
// ============================================================

/// 预处理：合并被 PDF 文本提取拆分的行
///
/// 问题：PDF 文本提取中，序号和描述常分为两行：
///   "2"          ← 纯序号行
///   "  3月 11日，原料分厂..."  ← 描述从下行开始
/// 合并为 "2 3月 11日，原料分厂..." 以便正则匹配。
fn pre_merge_split_lines(raw_lines: &[&str]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    let approx_idx_re = Regex::new(r"^\s*\d{1,2}\s*$").unwrap();

    let len = raw_lines.len();
    let mut i = 0;
    while i < len {
        let line = raw_lines[i].trim();
        if approx_idx_re.is_match(line) && i + 1 < len {
            // parse().unwrap_or(0) — 正则已保证此处字符串为数字，unwrap_or 仅做安全兜底
            let idx_num: i32 = line.parse().unwrap_or(0);
            // 仅当 1..=99 的合法序号 + 下一行以日期开头时合并
            if idx_num >= 1 && idx_num <= 99
                && i + 1 < len
                && assessment_desc_re().is_match(raw_lines[i + 1].trim())
            {
                merged.push(format!("{} {}", line, raw_lines[i + 1].trim()));
                i += 2;
                continue;
            }
        }
        merged.push(raw_lines[i].to_string());
        i += 1;
    }

    merged
}

/// 从 extract_text 的原始文本中解析考核记录
fn extract_from_text(full_text: &str, no_merge: bool, config: &ParserConfig) -> Vec<AssessmentRecord> {
    let mut records: Vec<AssessmentRecord> = Vec::new();
    let raw_lines: Vec<&str> = full_text.split('\n').collect();

    debug!("原始行数: {}", raw_lines.len());

    // 预处理：合并序号与描述被 PDF 文本提取拆分到两行的情况
    let lines: Vec<String> = if no_merge {
        // --no-merge 调试模式：跳过合并
        raw_lines.iter().map(|s| s.to_string()).collect()
    } else {
        pre_merge_split_lines(&raw_lines)
    };

    debug!("合并后行数: {}", lines.len());

    let mut current_record_lines: Vec<String> = Vec::new();
    let mut stopped = false;

    for line in &lines {
        if stopped {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // 检测章节边界 —— 仅在已进入考核区域后（已发现记录或有待处理行）才停止
        if !records.is_empty() || !current_record_lines.is_empty() {
            let mut hit_boundary = false;
            for bp in boundary_patterns() {
                if bp.is_match(line) {
                    debug!(
                        "  边界检测停止解析: 行 '{}' 匹配模式 '{}'，已保存 {} 条记录",
                        line,
                        bp.as_str(),
                        records.len()
                    );
                    // 保存当前记录
                    if let Some(record) = build_text_record(&current_record_lines, config) {
                        records.push(record);
                    }
                    current_record_lines.clear();
                    stopped = true;
                    hit_boundary = true;
                    break;
                }
            }
            if hit_boundary {
                break;
            }
        }

        // 检查是否为考核记录行开头
        if record_line_re().is_match(line) {
            // 保存前一条记录
            if let Some(record) = build_text_record(&current_record_lines, config) {
                records.push(record);
            }
            current_record_lines = vec![line.to_string()];
        } else if !current_record_lines.is_empty() {
            // 续行：追加到当前记录
            let skip_re = Regex::new(r"^(第\d+页|编号|项目|合计|小计|总计|合同嘉奖|嘉奖)").unwrap();
            if !skip_re.is_match(line) {
                current_record_lines.push(line.to_string());
            }
        }
    }

    // 处理最后一条记录
    if !stopped && !current_record_lines.is_empty() {
        if let Some(record) = build_text_record(&current_record_lines, config) {
            records.push(record);
        }
    }

    records
}

/// 从文本通道的多行构建 AssessmentRecord
///
/// 流程：
/// 1. 解析序号（如 `"12 3月..."`）
/// 2. 过滤非考核行
/// 3. 提取条款子串并移除
/// 4. 移除末尾金额数字
/// 5. 合并多个空格
fn build_text_record(lines: &[String], config: &ParserConfig) -> Option<AssessmentRecord> {
    if lines.is_empty() {
        return None;
    }

    let full_line = lines.join(" ");

    // 解析序号（编译期保证的模式，Regex::new().unwrap() 安全）
    let idx_match = Regex::new(r"^(\d{1,2})\s+").unwrap();
    let caps = idx_match.captures(&full_line)?;
    let index: i32 = caps.get(1)?.as_str().parse().ok()?;

    if index < 1 || index > config.max_item_index {
        return None;
    }

    // 过滤非考核行
    for pattern in non_assessment_patterns() {
        if pattern.is_match(&full_line) {
            return None;
        }
    }

    // 提取描述（序号之后到条款之前的部分）
    let remainder = full_line[caps.get(0).unwrap().end()..].to_string();

    // 提取条款
    let mut clauses: Vec<String> = Vec::new();
    for pattern in clause_patterns() {
        for m in pattern.find_iter(&remainder) {
            clauses.push(m.as_str().to_string());
        }
    }

    // 条款回退：从描述中提取条款编号（中文数字、阿拉伯数字 + 顿号/点号）
    // 匹配示例："（二）"、"1."、"（3）"等
    if clauses.is_empty() {
        let fallback_clause_re =
            Regex::new(r"[（(][一二三四五六七八九十\d]+[）)]|\d+\.[\s\u{00A0}]").unwrap();
        for m in fallback_clause_re.find_iter(&remainder) {
            clauses.push(m.as_str().to_string());
        }
    }

    // 提取金额
    let amount = fee::extract_final_number(&remainder)?;
    if amount < config.min_assessment_amount {
        return None;
    }

    // 清理描述：
    // 1) 移除条款子串
    let mut desc = remainder.clone();
    for clause in &clauses {
        desc = desc.replace(clause.as_str(), "");
    }
    // 2) 移除末尾金额数字
    if let Ok(amount_re) = Regex::new(r"\d+(?:,\d+)*(?:\.\d+)??\s*$") {
        desc = amount_re.replace(&desc, "").to_string();
    }
    // 3) 合并多个空格
    if let Ok(ws_re) = Regex::new(r"\s+") {
        desc = ws_re.replace_all(&desc, "").to_string();
    }

    // 校验描述
    if !assessment_desc_re().is_match(&desc) {
        return None;
    }

    Some(AssessmentRecord::new(
        index,
        desc,
        if clauses.is_empty() { String::new() } else { clauses.join("；") },
        amount,
    ))
}

// ============================================================
// 合并去重（预留：当同时启用表格和文本解析通道时使用）
// ============================================================

/// 合并两个通道的结果并去重
///
/// 预留：当同时启用表格和文本解析通道时用于合并结果。
#[allow(dead_code)]
pub fn merge_deduplicate(
    table_records: &[AssessmentRecord],
    text_records: &[AssessmentRecord],
) -> Vec<AssessmentRecord> {
    if table_records.is_empty() {
        return text_records.to_vec();
    }

    let mut table_map: std::collections::BTreeMap<i32, Vec<&AssessmentRecord>> = std::collections::BTreeMap::new();
    for r in table_records {
        table_map.entry(r.index).or_default().push(r);
    }

    let mut result: Vec<AssessmentRecord> = table_records.to_vec();

    for text_r in text_records {
        if let Some(existing) = table_map.get(&text_r.index) {
            let too_small = existing.iter().any(|e| e.amount > 0.0 && text_r.amount < e.amount * 0.1);
            if too_small {
                debug!(
                    "  文本记录疑似条款编号（金额过小），跳过: 序号 {} ¥{} vs ¥{}",
                    text_r.index, text_r.amount, existing[0].amount
                );
                continue;
            }

            if !text_r.description.is_empty() {
                if let Some(existing_record) = result.iter_mut().find(|r| r.index == text_r.index) {
                    if text_r.description.len() > existing_record.description.len() {
                        existing_record.description = text_r.description.clone();
                        if !text_r.clause.is_empty() {
                            existing_record.clause = text_r.clause.clone();
                        }
                        existing_record.parse_source = "merged".to_string();
                        debug!("  文本通道补充描述: 序号 {}", text_r.index);
                    }
                }
            }
        } else {
            let mut new_record = text_r.clone();
            new_record.parse_source = "text_only".to_string();
            result.push(new_record);
            debug!("  文本通道补充: 序号 {}", text_r.index);
        }
    }

    result
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_merge_split_lines() {
        let input = vec![
            "2",
            "  3月 11日，原料分厂...",
            "违反规定",
            "100.00"
        ];
        let result = pre_merge_split_lines(&input);
        // 两行均经过 trim()，中间仅一个空格
        assert_eq!(result[0], "2 3月 11日，原料分厂...");
    }
}