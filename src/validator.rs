//! 金额闭环校验模块
//! ===============
//! 比对程序提取的总金额与 PDF 中声明的合计金额。
//! 
//! 校验等级:
//! - 0% 偏差: 通过，记录 INFO
//! - <5% 偏差: 警告，但仍生成 Excel
//! - ≥5% 偏差: 错误，Excel 中在末尾附加红字警告行

use log::{info, warn, error};

use crate::models::SettlementData;

/// 5% 偏差警告阈值
const DEVIATION_WARN_THRESHOLD: f64 = 0.05;

/// 执行金额闭环校验
pub fn validate_amounts(data: &mut SettlementData) -> bool {
    let extracted_total = data.total_assessment;
    
    let Some(pdf_stated) = data.pdf_stated_total else {
        warn!("⚠ 未能从 PDF 中提取合同考核合计金额，跳过金额校验");
        return true;
    };

    let deviation = (extracted_total - pdf_stated).abs();
    let deviation_pct = if pdf_stated > 0.0 { deviation / pdf_stated } else { 0.0 };

    data.amount_deviation_pct = deviation_pct;

    info!("=== 金额闭环校验 ===");
    info!("  PDF 声明合计: ¥{:,.2}", pdf_stated);
    info!("  程序提取合计: ¥{:,.2}", extracted_total);
    info!("  偏差: ¥{:,.2} ({:.2}%)", deviation, deviation_pct * 100.0);

    if deviation_pct == 0.0 {
        data.amount_match = true;
        info!("✅ 金额校验通过（完全匹配）");
        true
    } else if deviation_pct < DEVIATION_WARN_THRESHOLD {
        data.amount_match = true;
        warn!(
            "⚠ 金额存在 {:.2}% 偏差（< {:.0}%），仍在可接受范围内",
            deviation_pct * 100.0,
            DEVIATION_WARN_THRESHOLD * 100.0
        );
        true
    } else {
        data.amount_match = false;
        error!(
            "❌ 金额校验失败！偏差 {:.2}% 超过 {:.0}% 阈值。\n   PDF 声明: ¥{:,.2}\n   程序提取: ¥{:,.2}\n   差额: ¥{:,.2}\n   请检查 PDF 解析结果！",
            deviation_pct * 100.0,
            DEVIATION_WARN_THRESHOLD * 100.0,
            pdf_stated,
            extracted_total,
            deviation
        );
        false
    }
}

/// 生成校验摘要文本
pub fn generate_validation_summary(data: &SettlementData) -> String {
    let mut parts = Vec::new();

    parts.push("═".repeat(50));
    parts.push("结算单校验摘要".to_string());
    parts.push("═".repeat(50));

    if !data.contract_no.is_empty() {
        parts.push(format!("合同编号: {}", data.contract_no));
    }
    if !data.contract_name.is_empty() {
        parts.push(format!("合同名称: {}", data.contract_name));
    }
    if !data.work_period.is_empty() {
        parts.push(format!("作业时间: {}", data.work_period));
    }

    parts.push(format!("\n考核明细: {} 条记录", data.all_records.len()));
    parts.push(format!("程序提取合计: ¥{:,.2}", data.total_assessment));

    if let Some(pdf_stated) = data.pdf_stated_total {
        parts.push(format!("PDF 声明合计: ¥{:,.2}", pdf_stated));
        let status = if data.amount_match { "✅ 通过" } else { "❌ 失败" };
        parts.push(format!(
            "偏差: {:.2}% ({})",
            data.amount_deviation_pct * 100.0,
            status
        ));
    }

    if data.total_reward > 0.0 {
        parts.push(format!("嘉奖金额: ¥{:,.2}", data.total_reward));
    }

    if data.work_fee > 0.0 {
        parts.push(format!("作业费用: ¥{:,.2}", data.work_fee));
        let settlement = data.get_settlement_amount();
        parts.push(format!("当月结算费用: ¥{:,.2}", settlement));
    }

    parts.push("\n区域明细:".to_string());
    for (area_name, area_data) in &data.areas {
        if !area_data.records.is_empty() {
            parts.push(format!(
                "  {}: {} 条, 小计 ¥{:,.2}, 事业部 ¥{:,.2}",
                area_name,
                area_data.records.len(),
                area_data.subtotal,
                area_data.dept_amount
            ));
        }
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AreaData;

    #[test]
    fn test_validate_exact_match() {
        let mut data = SettlementData::new();
        data.total_assessment = 1000.0;
        data.pdf_stated_total = Some(1000.0);
        assert!(validate_amounts(&mut data));
        assert!(data.amount_match);
    }

    #[test]
    fn test_validate_small_deviation() {
        let mut data = SettlementData::new();
        data.total_assessment = 1020.0;
        data.pdf_stated_total = Some(1000.0); // 2% deviation
        assert!(validate_amounts(&mut data));
        assert!(data.amount_match);
    }

    #[test]
    fn test_validate_large_deviation() {
        let mut data = SettlementData::new();
        data.total_assessment = 1100.0;
        data.pdf_stated_total = Some(1000.0); // 10% deviation
        assert!(!validate_amounts(&mut data));
        assert!(!data.amount_match);
    }

    #[test]
    fn test_validate_no_pdf_total() {
        let mut data = SettlementData::new();
        data.total_assessment = 1000.0;
        data.pdf_stated_total = None;
        // Should pass when no PDF total available (can't validate)
        assert!(validate_amounts(&mut data));
    }
}