//! 自定义错误类型
//! ================
//! 提供统一的错误类型，便于调用方区分和处理不同的错误场景。

use thiserror::Error;

/// 兴达结算工具基础错误
#[derive(Error, Debug)]
pub enum XingDaError {
    #[error("PDF解析失败: {0}")]
    Parse(String),

    #[error("数据校验失败: {0}")]
    Validation(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("Excel生成失败: {0}")]
    ExcelWrite(String),

    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("PDF错误: {0}")]
    Pdf(String),

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