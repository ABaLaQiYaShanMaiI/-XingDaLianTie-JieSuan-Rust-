//! PDF 解析模块（文本通道）
//! ======================
//! 使用 pdf-extract 提取 PDF 文本内容，
//! 通过正则匹配从文本行中提取考核记录。

use std::path::Path;
use std::fs;

use log::{info, warn, debug};
use regex::Regex;

use crate::error::{Result, XingDaError};
use crate::models::{AssessmentRecord, SettlementData};
use crate::config::ParserConfig;

// ============================================================
// 懒加载正则（Lazy Statics）
// ============================================================

/// 考核记录描述前缀（共用，兼容空格情况）
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
pub fn parse_pdf(pdf_path: &str) -> Result<SettlementData> {
    info!("正在读取 PDF: {}", pdf_path);

    let path = Path::new(pdf_path);
    if !path.exists() {
        return Err(XingDaError::Parse(format!("PDF 文件不存在: {}", pdf_path)));
    }

    let mut data = SettlementData::new();
    data.pdf_path = Some(pdf_path.to_string());

    // --- 提取 PDF 文本 ---
    let full_text = extract_pdf_text(pdf_path)?;

    if full_text.trim().is_empty() {
        return Err(XingDaError::Parse(format!("PDF 无文本内容: {}", pdf_path)));
    }

    // 保存原始文本（用于 --dump-text）
    data.raw_text = full_text.clone();

    // --- 提取合同基本信息 ---
    extract_contract_info(&mut data, &full_text);

    // --- 提取费用信息 ---
    extract_fee_info(&mut data, &full_text);

    // --- 文本行提取考核记录 ---
    let config = ParserConfig::default();
    let records = extract_from_text(&full_text, &config);

    info!("文本通道提取 {} 条记录", records.len());

    data.all_records = records;

    Ok(data)
}

/// 使用 pdf-extract 提取 PDF 文本
fn extract_pdf_text(pdf_path: &str) -> Result<String> {
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
                    return Ok(all_text);
                }
            }
            Ok(text)
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
                warn!("PDF 无文本层，OCR 功能暂未在 Rust 版中实现（需外部工具支持）");
            }
            Ok(all_text)
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
// 费用信息提取
// ============================================================

/// 从文本中提取最后一个数字作为金额
fn extract_final_number(text: &str) -> Option<f64> {
    let num_re = Regex::new(r"(?<![-.\d])(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
    let matches: Vec<_> = num_re.captures_iter(text).collect();
    matches.last()
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().replace(",", "").parse::<f64>().ok())
}

/// 提取费用信息
fn extract_fee_info(data: &mut SettlementData, full_text: &str) {
    // 策略1: 从底部结算公式合计行提取
    let sum_re = Regex::new(r"合计\s+(\d[\d,.]*)\s+(\d[\d,.]*)\s+(\d[\d,.]*)\s+(\d[\d,.]*)").unwrap();
    if let Some(caps) = sum_re.captures(full_text) {
        data.work_fee = parse_amount(caps.get(1)).unwrap_or(0.0);
        data.pdf_stated_total = parse_amount(caps.get(2));
        data.total_reward = parse_amount(caps.get(3)).unwrap_or(0.0);
        data.settlement_amount = parse_amount(caps.get(4)).unwrap_or(0.0);
        info!("  作业费用: {:.2}", data.work_fee);
        if let Some(total) = data.pdf_stated_total {
            info!("  PDF 考核金额合计: {:.2}", total);
        }
        info!("  嘉奖金额: {:.2}", data.total_reward);
        info!("  当月结算费用: {:.2}", data.settlement_amount);
        return;
    }

    // 策略2: 单独匹配作业费用小计
    let work_subtotal_re = Regex::new(r"作业费用\s*\n(?:.*\n)*?小计\s*(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
    if let Some(caps) = work_subtotal_re.captures(full_text) {
        data.work_fee = parse_amount(caps.get(1)).unwrap_or(0.0);
    }

    // 策略3: 嘉奖金额多策略匹配
    data.total_reward = extract_reward_amount(full_text);

    // 策略4: PDF 底部考核金额合计
    let total_re = Regex::new(r"(?:考核金额合计|合同考核.*?小计|总[计和])\s*\n?\s*(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
    if let Some(caps) = total_re.captures(full_text) {
        data.pdf_stated_total = parse_amount(caps.get(1));
    }
    if data.pdf_stated_total.is_none() {
        let total_re2 = Regex::new(r"考核金额.*?(\d{1,3}(?:,\d{3})*(?:\.\d+)?)").unwrap();
        if let Some(caps) = total_re2.captures(full_text) {
            data.pdf_stated_total = parse_amount(caps.get(1));
        }
    }
    if data.pdf_stated_total.is_none() {
        // 取最大的金额作为推断
        let num_re = Regex::new(r"(\d{1,3}(?:,\d{3})*(?:\.\d+)?)").unwrap();
        let mut large_amounts: Vec<f64> = num_re
            .captures_iter(full_text)
            .filter_map(|caps| caps.get(1))
            .filter_map(|m| m.as_str().replace(",", "").parse::<f64>().ok())
            .filter(|&a| a > 10000.0)
            .collect();
        large_amounts.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        data.pdf_stated_total = large_amounts.into_iter().next();
        if let Some(total) = data.pdf_stated_total {
            info!("  PDF 推断考核合计: {:.2}", total);
        }
    }
    if let Some(total) = data.pdf_stated_total {
        info!("  PDF 声明考核合计: {:.2}", total);
    }

    // 当月结算费用
    if data.settlement_amount == 0.0 {
        let settlement_re = Regex::new(r"当月结算费用[：:]*\s*(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
        if let Some(caps) = settlement_re.captures(full_text) {
            data.settlement_amount = parse_amount(caps.get(1)).unwrap_or(0.0);
        }
    }

    info!("  作业费用: {:.2}", data.work_fee);
    info!("  嘉奖金额: {:.2}", data.total_reward);
}

fn parse_amount(m: Option<regex::Match<'_>>) -> Option<f64> {
    m.and_then(|m| m.as_str().replace(",", "").parse::<f64>().ok())
}

// ============================================================
// 嘉奖金额提取
// ============================================================

/// 提取嘉奖金额，按优先级尝试多种匹配策略
fn extract_reward_amount(full_text: &str) -> f64 {
    let num_re_str = r"(\d+(?:,\d+)*(?:\.\d+)?)";
    let opt_space = r"\s*\n?\s*";

    // 策略1: 嘉奖金额 \n A \n 小计 \n B
    let re1_str = format!(r"嘉奖金额{}{}{}小计{}{}", opt_space, num_re_str, opt_space, opt_space, num_re_str);
    if let Ok(re1) = Regex::new(&re1_str) {
        if let Some(caps) = re1.captures(full_text) {
            if let Some(m) = caps.get(2) {
                if let Ok(val) = m.as_str().replace(",", "").parse::<f64>() {
                    debug!("  嘉奖金额（策略1: 三行模式）: {:.2}", val);
                    return val;
                }
            }
        }
    }

    // 策略2: 嘉奖金额 后直接跟金额
    let re2_str = format!(r"嘉奖金额[：:]*{}{}", opt_space, num_re_str);
    if let Ok(re2) = Regex::new(&re2_str) {
        if let Some(caps) = re2.captures(full_text) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().replace(",", "").parse::<f64>() {
                    debug!("  嘉奖金额（策略2: 直接匹配）: {:.2}", val);
                    return val;
                }
            }
        }
    }

    // 策略3: 找到"嘉奖金额"行，在后续 5 行内找金额
    let lines: Vec<&str> = full_text.split('\n').collect();
    let num_re = Regex::new(num_re_str).unwrap();
    for (idx, line) in lines.iter().enumerate() {
        if line.contains("嘉奖金额") {
            // 从本行提取
            let all_nums: Vec<f64> = num_re
                .captures_iter(line)
                .filter_map(|caps| caps.get(1))
                .filter_map(|m| m.as_str().replace(",", "").parse::<f64>().ok())
                .collect();
            if let Some(&val) = all_nums.last() {
                if val > 10.0 {
                    debug!("  嘉奖金额（策略3a: 同行末位）: {:.2}", val);
                    return val;
                }
            }

            // 扫描后续行
            for offset in 1..=5 {
                if idx + offset >= lines.len() {
                    break;
                }
                let scan_line = lines[idx + offset].trim();
                if let Some(caps) = num_re.captures(scan_line) {
                    if let Some(m) = caps.get(1) {
                        if let Ok(val) = m.as_str().replace(",", "").parse::<f64>() {
                            debug!("  嘉奖金额（策略3b: {}行后）: {:.2}", offset, val);
                            return val;
                        }
                    }
                }
            }
            break;
        }
    }

    // 策略4: 在"嘉奖金额"到"考核金额合计"之间找最大数
    let section_re = Regex::new(r"嘉奖金额(.*?)(?:考核金额合计|合同考核)").unwrap();
    if let Some(caps) = section_re.captures(full_text) {
        if let Some(section_m) = caps.get(1) {
            let section = section_m.as_str();
            let mut amounts: Vec<f64> = num_re
                .captures_iter(section)
                .filter_map(|caps| caps.get(1))
                .filter_map(|m| m.as_str().replace(",", "").parse::<f64>().ok())
                .filter(|&a| a > 10.0)
                .collect();
            if !amounts.is_empty() {
                amounts.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
                let val = amounts[0];
                debug!("  嘉奖金额（策略4: 区间最大值）: {:.2}", val);
                return val;
            }
        }
    }

    debug!("  未提取到嘉奖金额");
    0.0
}

// ============================================================
// 文本行提取考核记录
// ============================================================

/// 从 extract_text 的原始文本中解析考核记录
fn extract_from_text(full_text: &str, config: &ParserConfig) -> Vec<AssessmentRecord> {
    let mut records: Vec<AssessmentRecord> = Vec::new();
    let lines: Vec<&str> = full_text.split('\n').collect();

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

        // 检测章节边界
        let mut hit_boundary = false;
        for bp in boundary_patterns() {
            if bp.is_match(line) {
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
            if stopped {
                break;
            }
            continue;
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
fn build_text_record(lines: &[String], config: &ParserConfig) -> Option<AssessmentRecord> {
    if lines.is_empty() {
        return None;
    }

    let full_line = lines.join(" ");

    // 解析序号
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

    // 提取金额
    let amount = extract_final_number(&remainder)?;
    if amount < config.min_assessment_amount {
        return None;
    }

    // 清理描述
    let mut desc = remainder.clone();
    for clause in &clauses {
        desc = desc.replace(clause.as_str(), "");
    }
    // 移除末尾金额数字
    if let Ok(amount_re) = Regex::new(r"\d+(?:,\d+)*(?:\.\d+)??\s*$") {
        desc = amount_re.replace(&desc, "").to_string();
    }
    // 清理多余空白
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
// 合并去重（当有表格通道时使用）
// ============================================================

/// 合并两个通道的结果并去重
#[allow(dead_code)]
pub fn merge_deduplicate(
    table_records: &[AssessmentRecord],
    text_records: &[AssessmentRecord],
) -> Vec<AssessmentRecord> {
    if table_records.is_empty() {
        return text_records.to_vec();
    }

    // 以序号为键建立查找表
    let mut table_map: std::collections::BTreeMap<i32, Vec<&AssessmentRecord>> = std::collections::BTreeMap::new();
    for r in table_records {
        table_map.entry(r.index).or_default().push(r);
    }

    let mut result: Vec<AssessmentRecord> = table_records.to_vec();

    for text_r in text_records {
        if let Some(existing) = table_map.get(&text_r.index) {
            // 检查金额是否过小（疑似条款编号）
            let too_small = existing.iter().any(|e| e.amount > 0.0 && text_r.amount < e.amount * 0.1);
            if too_small {
                debug!(
                    "  文本记录疑似条款编号（金额过小），跳过: 序号 {} ¥{} vs ¥{}",
                    text_r.index, text_r.amount, existing[0].amount
                );
                continue;
            }

            // 比较描述长度，取更完整的
            if !text_r.description.is_empty() {
                // 找到对应的可变引用
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
            // 表格通道没有的记录，补充
            let mut new_record = text_r.clone();
            new_record.parse_source = "text_only".to_string();
            result.push(new_record);
            debug!("  文本通道补充: 序号 {}", text_r.index);
        }
    }

    result
}