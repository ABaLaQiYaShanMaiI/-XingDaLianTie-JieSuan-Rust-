//! 嘉奖金额提取（多策略）
//!
//! 从 PDF 文本中按优先级尝试多种匹配策略提取嘉奖金额。

use log::debug;
use regex::Regex;

use crate::config::ParserConfig;

/// 提取嘉奖金额，按优先级尝试多种匹配策略
pub fn extract_reward_amount(full_text: &str, config: &ParserConfig) -> f64 {
    let num_re_str = r"(\d+(?:,\d+)*(?:\.\d+)?)";
    let opt_space = r"\s*\n*\s*";
    let scan_lines = config.reward_scan_lines;
    let min_threshold = config.reward_filter_threshold;

    // 策略1: 嘉奖金额 \n A \n 小计 \n B（三行跨行模式）
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

    // 策略2: 找到"嘉奖金额"行，优先取同行末尾数字，再向下扫描（行扫描策略）
    // 注意：同行上仅取"嘉奖金额"之后、"考核金额合计/小计"之前的最后一个 > threshold 金额
    let lines: Vec<&str> = full_text.split('\n').collect();
    let num_re = Regex::new(num_re_str).unwrap();
    let boundary_end = Regex::new(r"考核金额合计|合同考核|小计").unwrap();
    for (idx, line) in lines.iter().enumerate() {
        if line.contains("嘉奖金额") {
            // 从本行提取：仅取嘉奖金额位置之后、边界词之前的数字
            if let Some(start_pos) = line.find("嘉奖金额") {
                let after_jj = &line[start_pos..];
                // 截断到边界词
                let search_text = if let Some(boundary_match) = boundary_end.find(after_jj) {
                    &after_jj[..boundary_match.start()]
                } else {
                    after_jj
                };
                let all_nums: Vec<f64> = num_re
                    .captures_iter(search_text)
                    .filter_map(|caps| caps.get(1))
                    .filter_map(|m| m.as_str().replace(",", "").parse::<f64>().ok())
                    .collect();
                if let Some(&val) = all_nums.last() {
                    if val > min_threshold {
                        debug!("  嘉奖金额（策略2a: 同行末位）: {:.2}", val);
                        return val;
                    }
                }
            }

            // 扫描后续行
            for offset in 1..=scan_lines {
                if idx + offset >= lines.len() {
                    break;
                }
                let scan_line = lines[idx + offset].trim();
                if let Some(caps) = num_re.captures(scan_line) {
                    if let Some(m) = caps.get(1) {
                        if let Ok(val) = m.as_str().replace(",", "").parse::<f64>() {
                            debug!("  嘉奖金额（策略2b: {}行后）: {:.2}", offset, val);
                            return val;
                        }
                    }
                }
            }
            break;
        }
    }

    // 策略3: 嘉奖金额 后直接跟金额（简单正则，回退用）
    let re2_str = format!(r"嘉奖金额[：:]*{}{}", opt_space, num_re_str);
    if let Ok(re2) = Regex::new(&re2_str) {
        if let Some(caps) = re2.captures(full_text) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().replace(",", "").parse::<f64>() {
                    debug!("  嘉奖金额（策略3: 直接匹配）: {:.2}", val);
                    return val;
                }
            }
        }
    }

    // 策略4: 在"嘉奖金额"到"考核金额合计"之间找最大数
    let section_re = Regex::new(r"嘉奖金额(.*?)(?:考核金额合计|合同考核)").unwrap();
    if let Some(caps) = section_re.captures(full_text) {
        if let Some(section_m) = caps.get(1) {
            let section = section_m.as_str();
            if let Some(val) = num_re
                .captures_iter(section)
                .filter_map(|caps| caps.get(1))
                .filter_map(|m| m.as_str().replace(",", "").parse::<f64>().ok())
                .filter(|&a| a > min_threshold)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            {
                debug!("  嘉奖金额（策略4: 区间最大值）: {:.2}", val);
                return val;
            }
        }
    }

    debug!("  未提取到嘉奖金额");
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ParserConfig {
        ParserConfig::default()
    }

    #[test]
    fn test_reward_strategy1_three_line() {
        let text = "嘉奖金额\n\n5000.00\n\n小计\n\n3000.00";
        let val = extract_reward_amount(text, &default_config());
        assert_eq!(val, 3000.00, "策略1应匹配小计后的金额3000.00");
    }

    #[test]
    fn test_reward_strategy2_direct_match() {
        let text = "嘉奖金额：12,500.00";
        let val = extract_reward_amount(text, &default_config());
        assert_eq!(val, 12500.00, "策略2应直接匹配冒号后金额");
    }

    #[test]
    fn test_reward_strategy3_peer_last() {
        let text = "嘉奖金额 200.00 500.00 800.00";
        let val = extract_reward_amount(text, &default_config());
        assert_eq!(val, 800.00, "策略3a应取同行最后一个>10的金额");
    }

    #[test]
    fn test_reward_strategy3b_scan_subsequent_lines() {
        let text = "嘉奖金额\n无关行\n另一行\n第三行\n第四行\n1500.50";
        let val = extract_reward_amount(text, &default_config());
        assert_eq!(val, 1500.50);
    }

    #[test]
    fn test_reward_strategy4_section_max() {
        let text = "嘉奖金额 50.00 200.00 考核金额合计 100000.00";
        let val = extract_reward_amount(text, &default_config());
        assert_eq!(val, 200.00, "策略4应在区间内取最大金额");
    }

    #[test]
    fn test_reward_no_match() {
        let text = "无相关内容";
        let val = extract_reward_amount(text, &default_config());
        assert_eq!(val, 0.0);
    }
}