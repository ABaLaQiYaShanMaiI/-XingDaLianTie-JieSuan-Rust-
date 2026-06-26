//! 数据模型：AssessmentRecord → AreaData → SettlementData

use std::collections::BTreeMap;

/// 考核记录
#[derive(Debug, Clone)]
pub struct AssessmentRecord {
    /// 序号
    pub index: i32,
    /// 考核事项描述
    pub description: String,
    /// 条款
    pub clause: String,
    /// 考核金额
    pub amount: f64,
    /// 归属作业区
    pub area: String,
    /// 解析来源: "table" 或 "text"
    pub parse_source: String,
    /// 解析警告
    pub parse_warnings: Vec<String>,
}

impl AssessmentRecord {
    pub fn new(index: i32, description: String, clause: String, amount: f64) -> Self {
        Self {
            index,
            description,
            clause,
            amount,
            area: String::new(),
            parse_source: String::new(),
            parse_warnings: Vec::new(),
        }
    }
}

/// 作业区数据
#[derive(Debug, Clone)]
pub struct AreaData {
    /// 区域名称
    pub name: String,
    /// 考核记录列表
    pub records: Vec<AssessmentRecord>,
    /// 小计
    pub subtotal: f64,
    /// 事业部考核金额
    pub dept_amount: f64,
}

impl AreaData {
    pub fn new(name: String) -> Self {
        Self {
            name,
            records: Vec::new(),
            subtotal: 0.0,
            dept_amount: 0.0,
        }
    }

    /// 添加考核记录
    pub fn add_record(&mut self, record: AssessmentRecord) {
        self.records.push(record);
    }

    /// 计算小计和事业部考核金额，并按考核金额降序排列
    pub fn calculate(&mut self, ratio: f64) {
        self.subtotal = self.records.iter().map(|r| r.amount).sum();
        // 事业部考核金额 = 小计 × ratio，保留两位小数（四舍五入）
        self.dept_amount = (self.subtotal * ratio * 100.0).round() / 100.0;
        self.records.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
    }
}

/// 结算单完整数据
#[derive(Debug, Clone)]
pub struct SettlementData {
    /// 合同编号
    pub contract_no: String,
    /// 合同名称
    pub contract_name: String,
    /// 作业时间
    pub work_period: String,
    /// 月份标签
    pub month_label: String,

    /// 作业费用
    pub work_fee: f64,
    /// 考核金额合计
    pub total_assessment: f64,
    /// 嘉奖金额合计
    pub total_reward: f64,
    /// 当月结算费用
    pub settlement_amount: f64,

    /// 全部考核记录
    pub all_records: Vec<AssessmentRecord>,
    /// 被过滤的记录及原因
    pub filtered_records: Vec<(AssessmentRecord, String)>,
    /// 按区域分组的记录
    pub areas: BTreeMap<String, AreaData>,

    /// PDF 中声明的合计金额
    pub pdf_stated_total: Option<f64>,
    /// 金额是否匹配
    pub amount_match: bool,
    /// 偏差百分比
    pub amount_deviation_pct: f64,

    /// PDF 路径（用于文件命名）
    pub pdf_path: Option<String>,
    /// 是否来自 OCR
    pub from_ocr: bool,
    /// 原始 PDF 文本内容（用于 --dump-text）
    pub raw_text: String,
}

impl SettlementData {
    pub fn new() -> Self {
        Self {
            contract_no: String::new(),
            contract_name: String::new(),
            work_period: String::new(),
            month_label: String::new(),
            work_fee: 0.0,
            total_assessment: 0.0,
            total_reward: 0.0,
            settlement_amount: 0.0,
            all_records: Vec::new(),
            filtered_records: Vec::new(),
            areas: BTreeMap::new(),
            pdf_stated_total: None,
            amount_match: true,
            amount_deviation_pct: 0.0,
        pdf_path: None,
        from_ocr: false,
        raw_text: String::new(),
    }
    }

    /// 计算汇总金额
    pub fn calculate_totals(&mut self) {
        self.total_assessment = self.areas.values().map(|a| a.subtotal).sum();
    }

    /// 当月结算费用 = 作业费用 - 考核金额合计 + 嘉奖金额合计
    pub fn get_settlement_amount(&self) -> f64 {
        self.work_fee - self.total_assessment + self.total_reward
    }
}

impl Default for SettlementData {
    fn default() -> Self {
        Self::new()
    }
}