//! 基于配置文件规则动态分类，替代硬编码 if-else。

use log::{info, warn};

use crate::config::ClassifyRules;
use crate::models::{AssessmentRecord, AreaData, SettlementData};

/// 对所有考核记录进行分类
pub fn classify_records(data: &mut SettlementData, rules: &ClassifyRules) {
    let department_ratio = rules.department_ratio;

    // 逐条分类（AreaData 在分类时动态创建，无需预分配）
    for i in 0..data.all_records.len() {
        let area_name = classify_one(&data.all_records[i], &rules.areas);
        data.all_records[i].area = area_name.clone();

        // 确保区域一定存在（classify_one 可能返回「未分类」等未在 rules.areas 中预定义的区域名）
        data.areas
            .entry(area_name.clone())
            .or_insert_with(|| AreaData::new(area_name));

        if let Some(area) = data.areas.get_mut(&data.all_records[i].area) {
            area.add_record(data.all_records[i].clone());
        }
    }

    // 计算各区域小计
    for area in data.areas.values_mut() {
        area.calculate(department_ratio);
    }

    data.calculate_totals();

    // 输出分类摘要
    info!("=== 分类结果 ===");
    for name in &rules.area_order {
        if let Some(area_data) = data.areas.get(name) {
            let sub: f64 = area_data.records.iter().map(|r| r.amount).sum();
            info!("  {}: {} 条，小计 ¥{:.2}", name, area_data.records.len(), sub);
        }
    }

    // 检查未分类区域
    if let Some(unclassified) = data.areas.get("未分类") {
        if !unclassified.records.is_empty() {
            let indices: Vec<i32> = unclassified.records.iter().map(|r| r.index).collect();
            warn!(
                "⚠ 有 {} 条记录无法分类，归入「未分类」: {:?}",
                unclassified.records.len(),
                indices
            );
        }
    }
}

/// 按配置规则对单条记录分类
///
/// 前置条件：调用方须确保 `AreaRule` 已通过 `compile_regexes()` 初始化，
/// 否则 `compiled_equipment_re` / `compiled_pattern_re` 均为空，会导致前缀/正则匹配失效。
fn classify_one(record: &AssessmentRecord, areas: &[crate::config::AreaRule]) -> String {
    let idx = record.index;
    let desc = &record.description;

    for area_cfg in areas {
        // 跳过无匹配条件的区域规则（如「未分类」兜底），
        // 最终未命中任何规则时在函数末尾统一归入「未分类」
        if !area_cfg.has_match_rules {
            continue;
        }

        // --- 规则1: 精确序号匹配 ---
        if area_cfg.item_numbers.contains(&idx) {
            return area_cfg.name.clone();
        }

        // --- 规则2: 关键词匹配 ---
        for kw in &area_cfg.keywords {
            if desc.contains(kw.as_str()) {
                return area_cfg.name.clone();
            }
        }

        // --- 规则3: 设备编号前缀匹配 ---
        for cre in &area_cfg.compiled_equipment_re {
            if cre.is_match(desc) {
                return area_cfg.name.clone();
            }
        }

        // --- 规则4: 描述文本正则匹配 ---
        for pre in &area_cfg.compiled_pattern_re {
            if pre.is_match(desc) {
                return area_cfg.name.clone();
            }
        }
    }

    // 所有规则都未命中 → 归入"未分类"
    "未分类".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AreaRule;

    /// 使用 compile_regexes() 构建测试规则，确保与生产代码逻辑一致
    fn test_rules() -> Vec<AreaRule> {
        let mut areas = vec![
            AreaRule::new(
                "事业部".into(),
                1,
                vec![],
                vec![],
                vec![],
                vec!["协力安全管理工作方案.*落实".into(), "合同评价.*排名".into()],
            ),
            AreaRule::new(
                "供矿作业区".into(),
                2,
                vec![],
                vec!["供矿".into(), "翻车".into(), "球团".into()],
                vec![],
                vec![],
            ),
            AreaRule::new(
                "煤库作业区".into(),
                3,
                vec![],
                vec!["煤库".into(), "原煤仓".into(), "原煤".into(), "卸煤间".into()],
                vec!["M".into()],
                vec![],
            ),
            AreaRule::new(
                "原料分厂作业区".into(),
                4,
                vec![],
                vec!["原料分厂".into(), "输入作业区".into(), "原料班".into()],
                vec!["B".into(), "E".into(), "F".into(), "K".into(), "N".into(), "C".into()],
                vec![],
            ),
            AreaRule::new(
                "未分类".into(),
                99,
                vec![],
                vec![],
                vec![],
                vec![],
            ),
        ];
        for area in &mut areas {
            area.compile_regexes().unwrap();
        }
        areas
    }

    #[test]
    fn test_classify_keyword_match() {
        let rules = test_rules();
        let record = AssessmentRecord::new(1, "供矿系统安全检查".into(), "".into(), 100.0);
        assert_eq!(classify_one(&record, &rules), "供矿作业区");
    }

    #[test]
    fn test_classify_equipment_prefix() {
        let rules = test_rules();
        let record = AssessmentRecord::new(2, "M3皮带机故障".into(), "".into(), 200.0);
        assert_eq!(classify_one(&record, &rules), "煤库作业区");
    }

    #[test]
    fn test_classify_description_pattern() {
        let rules = test_rules();
        let record = AssessmentRecord::new(3, "协力安全管理工作方案未落实".into(), "".into(), 300.0);
        assert_eq!(classify_one(&record, &rules), "事业部");
    }

    #[test]
    fn test_classify_unclassified() {
        let rules = test_rules();
        let record = AssessmentRecord::new(99, "其他事项".into(), "".into(), 50.0);
        assert_eq!(classify_one(&record, &rules), "未分类");
    }
}