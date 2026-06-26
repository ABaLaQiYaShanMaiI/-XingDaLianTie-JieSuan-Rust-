//! 统一的错误类型，便于调用方区分和处理不同的错误场景。

use thiserror::Error;

/// 兴达结算工具基础错误
#[derive(Error, Debug)]
pub enum XingDaError {
    /// PDF 解析阶段出现的错误（文件损坏、格式不支持、OCR 失败等）
    #[error("PDF解析失败: {0}")]
    Parse(String),

    /// 金额闭环校验失败（提取合计与 PDF 声明合计偏差超出阈值）
    #[error("数据校验失败: {0}")]
    Validation(String),

    /// 配置文件加载或解析错误（YAML 格式错误、文件缺失等）
    #[error("配置错误: {0}")]
    Config(String),

    /// Excel 写入阶段错误（磁盘满、权限不足、格式错误等）
    #[error("Excel生成失败: {0}")]
    ExcelWrite(String),

    /// 文件系统 I/O 错误
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    /// PDF 文件相关的错误（无文本层、OCR 工具缺失等）
    #[error("PDF错误: {0}")]
    Pdf(String),

    /// 透明转换 anyhow::Error，用于捕获未预见的第三方库错误
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, XingDaError>;

impl From<regex::Error> for XingDaError {
    fn from(e: regex::Error) -> Self {
        XingDaError::Config(format!("正则编译错误: {}", e))
    }
}

impl From<serde_yaml::Error> for XingDaError {
    fn from(e: serde_yaml::Error) -> Self {
        XingDaError::Config(format!("YAML解析错误: {}", e))
    }
}

impl From<rust_xlsxwriter::XlsxError> for XingDaError {
    fn from(e: rust_xlsxwriter::XlsxError) -> Self {
        XingDaError::ExcelWrite(format!("{}", e))
    }
}