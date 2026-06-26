//! 费用信息提取
//!
//! 从 PDF 文本中按多种策略提取作业费用、嘉奖金额、考核金额合计、当月结算费用。

use log::{info, debug};
use regex::Regex;

use crate::config::ParserConfig;
use crate::models::SettlementData;

use super::reward::extract_reward_amount;

/// 从文本中提取最后一个数字作为金额
pub fn extract_final_number(text: &str) -> Option<f64> {
    let num_re = Regex::new(r"\d+(?:,\d+)*(?:\.\d+)?").unwrap();
    num_re.captures_iter(text)
        .last()
        .and_then(|caps| caps.get(0))
        .and_then(|m| m.as_str().replace(",", "").parse::<f64>().ok())
}

pub fn parse_amount(m: Option<regex::Match<'_>>) -> Option<f64> {
    m.and_then(|m| m.as_str().replace(",", "").parse::<f64>().ok())
}

/// 提取费用信息
pub fn extract_fee_info(data: &mut SettlementData, full_text: &str, config: &ParserConfig) {
    // 策略1: 从底部结算公式合计行提取
    // 仅当关键字段（pdf_stated_total + work_fee）都有效时才采用，否则回退到后续策略
    let sum_re = Regex::new(r"合计\s+(\d[\d,.]*)\s+(\d[\d,.]*)\s+(\d[\d,.]*)\s+(\d[\d,.]*)").unwrap();
    if let Some(caps) = sum_re.captures(full_text) {
        let work_fee = parse_amount(caps.get(1)).unwrap_or(0.0);
        let pdf_stated = parse_amount(caps.get(2));
        let total_reward = parse_amount(caps.get(3)).unwrap_or(0.0);
        let settlement = parse_amount(caps.get(4)).unwrap_or(0.0);

        // 有效性检查：关键字段缺失则继续后续策略
        if pdf_stated.is_some() && work_fee > 0.0 {
            data.work_fee = work_fee;
            data.pdf_stated_total = pdf_stated;
            data.total_reward = total_reward;
            data.settlement_amount = settlement;
            info!("  作业费用: {:.2}", data.work_fee);
            if let Some(total) = data.pdf_stated_total {
                info!("  PDF 考核金额合计: {:.2}", total);
            }
            info!("  嘉奖金额: {:.2}", data.total_reward);
            info!("  当月结算费用: {:.2}", data.settlement_amount);
            return;
        }
        debug!("  策略1 合计行关键字段无效（work_fee={}, pdf_total={:?}），回退后续策略", work_fee, pdf_stated);
    }

    // 策略2: 单独匹配作业费用小计
    let work_subtotal_re = Regex::new(r"作业费用\s*\n(?:.*\n)*?小计\s*(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
    if let Some(caps) = work_subtotal_re.captures(full_text) {
        data.work_fee = parse_amount(caps.get(1)).unwrap_or(0.0);
    }

    // 策略3: 嘉奖金额多策略匹配
    data.total_reward = extract_reward_amount(full_text, config);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ParserConfig {
        ParserConfig::default()
    }

    #[test]
    fn test_extract_final_number() {
        assert_eq!(extract_final_number("违反规定 100.00"), Some(100.00));
        assert_eq!(extract_final_number("无金额"), None);
    }

    #[test]
    fn test_extract_fee_info_strategy1_sum_row() {
        let mut data = SettlementData::new();
        let text = "合同编号：SC-2024-001\n作业时间：2024年3月\n合计 50000.00 12000.00 800.00 38800.00";
        extract_fee_info(&mut data, text, &default_config());
        assert_eq!(data.work_fee, 50000.00);
        assert_eq!(data.pdf_stated_total, Some(12000.00));
        assert_eq!(data.total_reward, 800.00);
        assert_eq!(data.settlement_amount, 38800.00);
    }

    #[test]
    fn test_extract_fee_info_strategy2_work_subtotal_only() {
        let mut data = SettlementData::new();
        let text = "作业费用\n小计 45000.00";
        extract_fee_info(&mut data, text, &default_config());
        assert_eq!(data.work_fee, 45000.00);
    }

    #[test]
    fn test_extract_fee_info_settlement_fallback() {
        let mut data = SettlementData::new();
        // 无合计行 → 靠单独匹配当月结算费用
        let text = "作业费用\n小计 43000.00\n当月结算费用： 41,500.00";
        extract_fee_info(&mut data, text, &default_config());
        assert_eq!(data.settlement_amount, 41500.00);
    }

    #[test]
    fn test_extract_fee_info_reward_integration() {
        let mut data = SettlementData::new();
        let text = "嘉奖金额：5,000.00\n作业费用\n小计 40000.00";
        extract_fee_info(&mut data, text, &default_config());
        assert_eq!(data.total_reward, 5000.00);
        assert_eq!(data.work_fee, 40000.00);
    }
}