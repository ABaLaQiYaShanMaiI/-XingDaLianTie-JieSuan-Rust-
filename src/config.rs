//! 配置加载模块
//! ===========
//! 支持从 YAML 文件加载分类规则及 Excel 样式配置，
//! 未指定时使用内置默认值。

use std::fs;
use std::path::PathBuf;

use log::{info, warn};
use regex::Regex;

use crate::error::{Result, XingDaError};

// ============================================================
// 默认配置（硬编码兜底）
// ============================================================

/// 分类规则配置
#[derive(Debug, Clone)]
pub struct ClassifyRules {
    /// 区域规则列表（已按 priority 排序）
    pub areas: Vec<AreaRule>,
    /// 事业部考核金额计算比例
    pub department_ratio: f64,
    /// Excel 输出区域顺序
    pub area_order: Vec<String>,
}

/// 单个区域规则
#[derive(Debug, Clone)]
pub struct AreaRule {
    pub name: String,
    pub priority: i32,
    pub item_numbers: Vec<i32>,
    pub keywords: Vec<String>,
    pub equipment_prefixes: Vec<String>,
    pub description_patterns: Vec<String>,
    // 预编译的正则（运行时填充）
    pub compiled_equipment_re: Vec<Regex>,
    pub compiled_pattern_re: Vec<Regex>,
}

impl AreaRule {
    pub fn compile_regexes(&mut self) -> Result<()> {
        self.compiled_equipment_re = self
            .equipment_prefixes
            .iter()
            .map(|p| Regex::new(&format!(r"{}\d+", regex::escape(p))))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        self.compiled_pattern_re = self
            .description_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(())
    }
}

/// Excel 样式配置
#[derive(Debug, Clone)]
pub struct ExcelStyle {
    pub font_name: String,
    pub font_size: f64,
    pub col_width_a: f64,
    pub col_width_b: f64,
    pub col_width_c: f64,
    pub header_row_height: f64,
}

/// Excel 样式预设（对应 --style 参数）
#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum StylePreset {
    /// 紧凑样式（小字体、窄列宽）
    Compact,
    /// 宽松样式（大字体、宽列宽）
    Wide,
}

impl ExcelStyle {
    /// 应用样式预设
    pub fn apply_preset(mut self, preset: StylePreset) -> Self {
        match preset {
            StylePreset::Compact => {
                self.font_size = 12.0;
                self.col_width_a = 60.0;
                self.col_width_b = 30.0;
                self.col_width_c = 16.0;
                self.header_row_height = 30.0;
            }
            StylePreset::Wide => {
                self.font_size = 18.0;
                self.col_width_a = 100.0;
                self.col_width_b = 50.0;
                self.col_width_c = 28.0;
                self.header_row_height = 48.0;
            }
        }
        self
    }
}

impl Default for ExcelStyle {
    fn default() -> Self {
        Self {
            font_name: "宋体".to_string(),
            font_size: 16.0,
            col_width_a: 80.625,
            col_width_b: 40.625,
            col_width_c: 22.0,
            header_row_height: 40.5,
        }
    }
}

/// PDF 解析器默认常量
#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub max_item_index: i32,
    pub min_assessment_amount: f64,
    pub reward_scan_lines: usize,
    pub ocr_dpi: u32,
    pub reward_filter_threshold: f64,
    /// OCR 引擎选择（默认 Tesseract）
    pub ocr_engine: OcrEngine,
    /// OCR 语言包（默认 chi_sim）
    pub ocr_lang: String,
    /// Tesseract PSM 模式 (3-13，默认 6)
    pub tesseract_psm: u8,
}

/// OCR 引擎类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrEngine {
    Tesseract,
    // EasyOCR,   // 未来扩展
    // PaddleOCR, // 未来扩展
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            max_item_index: 99,
            min_assessment_amount: 1.0,
            reward_scan_lines: 5,
            ocr_dpi: 300,
            reward_filter_threshold: 10.0,
            ocr_engine: OcrEngine::Tesseract,
            ocr_lang: "chi_sim".to_string(),
            tesseract_psm: 6,
        }
    }
}

// ============================================================
// YAML 反序列化结构
// ============================================================

#[derive(Debug, serde::Deserialize)]
struct RawRules {
    areas: Option<Vec<RawAreaRule>>,
    department_ratio: Option<f64>,
    area_order: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct RawAreaRule {
    name: String,
    priority: Option<i32>,
    #[serde(rename = "match")]
    match_rules: Option<RawMatchRules>,
}

#[derive(Debug, serde::Deserialize)]
struct RawMatchRules {
    item_numbers: Option<Vec<i32>>,
    keywords: Option<Vec<String>>,
    equipment_prefixes: Option<Vec<String>>,
    description_patterns: Option<Vec<String>>,
}

// ============================================================
// 默认规则构建
// ============================================================

/// 内置默认分类规则（硬编码兜底）
pub fn default_rules() -> ClassifyRules {
    let areas = vec![
        AreaRule {
            name: "事业部".to_string(),
            priority: 1,
            item_numbers: vec![],
            keywords: vec![],
            equipment_prefixes: vec![],
            description_patterns: vec![
                "协力安全管理工作方案.*落实".to_string(),
                "合同评价.*排名".to_string(),
            ],
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
        },
        AreaRule {
            name: "供矿作业区".to_string(),
            priority: 2,
            item_numbers: vec![],
            keywords: vec![
                "供矿".to_string(),
                "翻车".to_string(),
                "球团".to_string(),
            ],
            equipment_prefixes: vec![],
            description_patterns: vec![],
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
        },
        AreaRule {
            name: "煤库作业区".to_string(),
            priority: 3,
            item_numbers: vec![],
            keywords: vec![
                "煤库".to_string(),
                "原煤仓".to_string(),
                "原煤".to_string(),
                "卸煤间".to_string(),
            ],
            equipment_prefixes: vec!["M".to_string()],
            description_patterns: vec![],
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
        },
        AreaRule {
            name: "原料分厂作业区".to_string(),
            priority: 4,
            item_numbers: vec![],
            keywords: vec![
                "原料分厂".to_string(),
                "输入作业区".to_string(),
                "输入区域".to_string(),
                "原料区域".to_string(),
                "原料输入".to_string(),
                "原料班".to_string(),
                "协力系统".to_string(),
                "兴达原料作业区".to_string(),
            ],
            equipment_prefixes: vec![
                "B".to_string(),
                "E".to_string(),
                "F".to_string(),
                "K".to_string(),
                "N".to_string(),
                "C".to_string(),
            ],
            description_patterns: vec![],
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
        },
        AreaRule {
            name: "未分类".to_string(),
            priority: 99,
            item_numbers: vec![],
            keywords: vec![],
            equipment_prefixes: vec![],
            description_patterns: vec![],
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
        },
    ];

    let area_order = areas.iter().map(|a| a.name.clone()).collect();

    ClassifyRules {
        areas,
        department_ratio: 0.01,
        area_order,
    }
}

// ============================================================
// 加载函数
// ============================================================

/// 编译时嵌入的 classify_rules.yaml 内容（自包含回退）
const EMBEDDED_RULES_YAML: &str = include_str!("../classify_rules.yaml");

/// 获取默认分类规则文件的候选路径列表
fn get_default_rules_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. EXE 所在目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            paths.push(exe_dir.join("classify_rules.yaml"));
        }
    }

    // 2. 当前工作目录
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("classify_rules.yaml"));
    }

    // 3. 项目根目录（开发模式：相对于 src/ 的上层）
    // 嵌入的配置文件会在同目录被找到

    paths
}

/// 加载分类规则配置
pub fn load_rules(rules_path: Option<&str>) -> Result<ClassifyRules> {
    // 确定实际使用的规则文件路径
    let actual_path = if let Some(path) = rules_path {
        Some(PathBuf::from(path))
    } else {
        // 尝试从默认路径加载
        get_default_rules_paths()
            .into_iter()
            .find(|p| p.exists())
    };

    let raw_rules = match &actual_path {
        Some(path) => {
            info!("已加载分类规则配置: {:?}", path);
            let content = fs::read_to_string(path)
                .map_err(|e| XingDaError::Config(format!("无法读取配置文件 {:?}: {}", path, e)))?;
            serde_yaml::from_str::<RawRules>(&content)
                .unwrap_or_else(|e| {
                    warn!("YAML 配置解析失败: {}，回退到内置默认规则", e);
                    return default_rules_as_raw();
                })
        }
        None => {
            info!("未找到分类规则配置文件，使用编译时嵌入的规则");
            // 优先使用 compile-time 嵌入的 classify_rules.yaml
            serde_yaml::from_str::<RawRules>(EMBEDDED_RULES_YAML)
                .unwrap_or_else(|e| {
                    warn!("嵌入规则解析失败: {}，回退到硬编码默认规则", e);
                    default_rules_as_raw()
                })
        }
    };

    // 转换为 ClassifyRules
    build_rules_from_raw(raw_rules)
}

fn default_rules_as_raw() -> RawRules {
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

fn build_rules_from_raw(raw: RawRules) -> Result<ClassifyRules> {
    // 处理 areas
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
                AreaRule {
                    name: ra.name,
                    priority: ra.priority.unwrap_or(99),
                    item_numbers: match_rules.item_numbers.unwrap_or_default(),
                    keywords: match_rules.keywords.unwrap_or_default(),
                    equipment_prefixes: match_rules.equipment_prefixes.unwrap_or_default(),
                    description_patterns: match_rules.description_patterns.unwrap_or_default(),
                    compiled_equipment_re: vec![],
                    compiled_pattern_re: vec![],
                }
            })
            .collect(),
        None => {
            vec![AreaRule {
                name: "未分类".to_string(),
                priority: 99,
                item_numbers: vec![],
                keywords: vec![],
                equipment_prefixes: vec![],
                description_patterns: vec![],
                compiled_equipment_re: vec![],
                compiled_pattern_re: vec![],
            }]
        }
    };

    // 按 priority 排序
    areas.sort_by_key(|a| a.priority);

    // 确保有兜底规则（未分类）
    let has_fallback = areas.iter().any(|a| a.name == "未分类");
    if !has_fallback {
        areas.push(AreaRule {
            name: "未分类".to_string(),
            priority: 99,
            item_numbers: vec![],
            keywords: vec![],
            equipment_prefixes: vec![],
            description_patterns: vec![],
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
        });
    }

    // 编译正则
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

/// 加载 Excel 样式配置（当前使用默认值）
pub fn load_excel_style() -> ExcelStyle {
    ExcelStyle::default()
}