//! 分类规则与 Excel 样式配置加载，未指定时使用内置默认值。

mod rules;

use std::fs;
use std::path::PathBuf;

use log::{info, warn};
use regex::Regex;

use crate::error::{Result, XingDaError};

// ============================================================
// 核心配置类型
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
    /// 是否包含任何匹配条件（加载时预计算，避免 classify_one 每次重复判断）
    pub has_match_rules: bool,
}

impl AreaRule {
    /// 创建 AreaRule 并自动计算 `has_match_rules`
    pub fn new(
        name: String,
        priority: i32,
        item_numbers: Vec<i32>,
        keywords: Vec<String>,
        equipment_prefixes: Vec<String>,
        description_patterns: Vec<String>,
    ) -> Self {
        let has_match = !item_numbers.is_empty()
            || !keywords.is_empty()
            || !equipment_prefixes.is_empty()
            || !description_patterns.is_empty();
        Self {
            name,
            priority,
            item_numbers,
            keywords,
            equipment_prefixes,
            description_patterns,
            compiled_equipment_re: vec![],
            compiled_pattern_re: vec![],
            has_match_rules: has_match,
        }
    }

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
// 加载函数
// ============================================================

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

    paths
}

/// 加载分类规则配置
///
/// 优先级：用户指定路径 > EXE 同目录 classify_rules.yaml > 当前工作目录 classify_rules.yaml >
/// 编译时嵌入 YAML > 硬编码兜底（`default_rules_as_raw`）
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
            serde_yaml::from_str::<rules::RawRules>(&content)
                .unwrap_or_else(|e| {
                    warn!("YAML 配置解析失败: {}，回退到内置默认规则", e);
                    return rules::default_rules_as_raw();
                })
        }
        None => {
            info!("未找到分类规则配置文件，使用编译时嵌入的规则");
            // 优先使用 compile-time 嵌入的 classify_rules.yaml
            serde_yaml::from_str::<rules::RawRules>(rules::EMBEDDED_RULES_YAML)
                .unwrap_or_else(|e| {
                    warn!("嵌入规则解析失败: {}，回退到硬编码默认规则", e);
                    rules::default_rules_as_raw()
                })
        }
    };

    // 转换为 ClassifyRules
    rules::build_rules_from_raw(raw_rules)
}

/// 加载 Excel 样式配置（当前使用默认值）
pub fn load_excel_style() -> ExcelStyle {
    ExcelStyle::default()
}