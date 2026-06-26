//! PDF结算单转Excel明细工具。库入口，供集成测试和外部使用。

pub mod classifier;
pub mod cli;
pub mod config;
pub mod error;
pub mod excel_writer;
pub mod gui;
pub mod ocr;
pub mod models;
pub mod parser;
pub mod validator;