//! YAML 反序列化结构与默认规则构建

use crate::error::Result;

use super::{AreaRule, ClassifyRules};

// ============================================================
// YAML 反序列化结构
// ============================================================

#[derive(Debug, serde::Deserialize)]
pub struct RawRules {
    pub areas: Option<Vec<RawAreaRule>>,
    pub department_ratio: Option<f64>,
    pub area_order: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RawAreaRule {
    pub name: String,
    pub priority: Option<i32>,
    #[serde(rename = "match")]
    pub match_rules: Option<RawMatchRules>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RawMatchRules {
    pub item_numbers: Option<Vec<i32>>,
    pub keywords: Option<Vec<String>>,
    pub equipment_prefixes: Option<Vec<String>>,
    pub description_patterns: Option<Vec<String>>,
}

// ============================================================
// 默认规则构建
// ============================================================

/// 编译时嵌入的 classify_rules.yaml 内容（通过 `include_str!` 将 ../classify_rules.yaml 嵌入二进制，作为自包含回退）
pub const EMBEDDED_RULES_YAML: &str = include_str!("../../classify_rules.yaml");

/// 硬编码兜底规则
pub fn default_rules_as_raw() -> RawRules {
    RawRules {
        areas: Some(vec![
            RawAreaRule {
                name: "事业部".to_string(),
                priority: Some(1),
                match_rules: Some(RawMatchRules {
                    item_numbers: Some(vec![]),
                    keywords: None,
                    equipment_prefixes: None,
                    description_patterns: Some(vec![
                        "协力安全管理工作方案.*落实".to_string(),
                        "合同评价.*排名".to_string(),
                    ]),
                }),
            },
            RawAreaRule {
                name: "供矿作业区".to_string(),
                priority: Some(2),
                match_rules: Some(RawMatchRules {
                    item_numbers: None,
                    keywords: Some(vec!["供矿".to_string(), "翻车".to_string(), "球团".to_string()]),
                    equipment_prefixes: Some(vec![]),
                    description_patterns: None,
                }),
            },
            RawAreaRule {
                name: "煤库作业区".to_string(),
                priority: Some(3),
                match_rules: Some(RawMatchRules {
                    item_numbers: None,
                    keywords: Some(vec![
                        "煤库".to_string(),
                        "原煤仓".to_string(),
                        "原煤".to_string(),
                        "卸煤间".to_string(),
                    ]),
                    equipment_prefixes: Some(vec!["M".to_string()]),
                    description_patterns: None,
                }),
            },
            RawAreaRule {
                name: "原料分厂作业区".to_string(),
                priority: Some(4),
                match_rules: Some(RawMatchRules {
                    item_numbers: None,
                    keywords: Some(vec![
                        "原料分厂".to_string(),
                        "输入作业区".to_string(),
                        "输入区域".to_string(),
                        "原料区域".to_string(),
                        "原料输入".to_string(),
                        "原料班".to_string(),
                        "协力系统".to_string(),
                        "兴达原料作业区".to_string(),
                    ]),
                    equipment_prefixes: Some(vec![
                        "B".to_string(),
                        "E".to_string(),
                        "F".to_string(),
                        "K".to_string(),
                        "N".to_string(),
                        "C".to_string(),
                    ]),
                    description_patterns: None,
                }),
            },
            RawAreaRule {
                name: "未分类".to_string(),
                priority: Some(99),
                match_rules: Some(RawMatchRules {
                    item_numbers: None,
                    keywords: None,
                    equipment_prefixes: None,
                    description_patterns: None,
                }),
            },
        ]),
        department_ratio: Some(0.01),
        area_order: None,
    }
}

/// 从 RawRules 构建 ClassifyRules
pub fn build_rules_from_raw(raw: RawRules) -> Result<ClassifyRules> {
    let mut areas: Vec<AreaRule> = match raw.areas {
        Some(raw_areas) => raw_areas
            .into_iter()
            .map(|ra| {
                let match_rules = ra.match_rules.unwrap_or(RawMatchRules {
                    item_numbers: None,
                    keywords: None,
                    equipment_prefixes: None,
                    description_patterns: None,
                });
                AreaRule::new(
                    ra.name,
                    ra.priority.unwrap_or(99),
                    match_rules.item_numbers.unwrap_or_default(),
                    match_rules.keywords.unwrap_or_default(),
                    match_rules.equipment_prefixes.unwrap_or_default(),
                    match_rules.description_patterns.unwrap_or_default(),
                )
            })
            .collect(),
        None => {
            vec![AreaRule::new(
                "未分类".to_string(),
                99,
                vec![],
                vec![],
                vec![],
                vec![],
            )]
        }
    };

    areas.sort_by_key(|a| a.priority);

    // 确保有兜底规则（未分类）
    let has_fallback = areas.iter().any(|a| a.name == "未分类");
    if !has_fallback {
        areas.push(AreaRule::new(
            "未分类".to_string(),
            99,
            vec![],
            vec![],
            vec![],
            vec![],
        ));
    }

    for area in &mut areas {
        area.compile_regexes()?;
    }

    let department_ratio = raw.department_ratio.unwrap_or(0.01);
    let area_order = raw
        .area_order
        .unwrap_or_else(|| areas.iter().map(|a| a.name.clone()).collect());

    Ok(ClassifyRules {
        areas,
        department_ratio,
        area_order,
    })
}