//! Excel 生成模块
//! ==============
//! 生成格式化的 Excel 结算单明细文件。
//! 功能: 汇总信息区域 + 区考核概览 + 区域明细 + 校验失败警告行。

use std::path::Path;

use log::info;
use rust_xlsxwriter::{Format, Workbook, Worksheet, Color};

use crate::config::ExcelStyle;
use crate::error::{Result, XingDaError};
use crate::models::{SettlementData, AreaData};

/// 生成 Excel 结算单明细文件
pub fn generate_excel(
    data: &SettlementData,
    output_path: &str,
    area_order: &[String],
    style: &ExcelStyle,
    include_summary: bool,
) -> Result<()> {
    info!("正在生成 Excel: {}", output_path);

    // 确保输出目录存在
    if let Some(parent) = Path::new(output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            XingDaError::ExcelWrite(format!("无法创建输出目录: {}", e))
        })?;
    }

    let mut workbook = Workbook::new();

    // ---- 定义格式 ----
    // 标题字体格式（黑体）
    let header_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name);

    // 普通数据字体格式
    let data_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name);

    // 金额数字格式
    let amount_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_num_format("#,##0.00");

    // 红色错误格式
    let red_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_font_color(Color::Red);

    // 居中对齐格式
    let center_header_format = header_format.clone();
    // (rust_xlsxwriter 默认左对齐，我们通过列宽和内容来容处理)

    // 创建工作表
    let sheet_name = if data.month_label.is_empty() {
        "结算明细"
    } else {
        &data.month_label
    };
    let worksheet = workbook.add_worksheet();

    // 设置列宽
    worksheet.set_column_width(0, style.col_width_a)?;
    worksheet.set_column_width(1, style.col_width_b)?;
    worksheet.set_column_width(2, style.col_width_c)?;

    let mut current_row: u32 = 0;

    // --- 1. 汇总信息区域 ---
    if include_summary {
        current_row = write_summary_section(
            &worksheet,
            current_row,
            data,
            &header_format,
            &data_format,
            &amount_format,
        )?;
    }

    // --- 2. 区域明细 ---
    for (idx, area_name) in area_order.iter().enumerate() {
        if let Some(area_data) = data.areas.get(area_name) {
            if area_data.records.is_empty() {
                continue;
            }

            if idx > 0 {
                current_row += 1; // 区域间空行
            }

            current_row = write_area_section(
                &worksheet,
                current_row,
                area_data,
                &header_format,
                &data_format,
                &amount_format,
                style,
            )?;
        }
    }

    // --- 3. 校验失败警告行 ---
    if !data.amount_match {
        current_row += 1;
        let warning_text = format!(
            "⚠ 金额校验失败：PDF 声明合计 ¥{:,.2}，程序提取合计 ¥{:,.2}，偏差 {:.2}%",
            data.pdf_stated_total.unwrap_or(0.0),
            data.total_assessment,
            data.amount_deviation_pct * 100.0
        );
        worksheet.merge_range(
            current_row,
            0,
            current_row,
            2,
            &warning_text,
            &red_format,
        )?;
    }

    workbook.save(output_path)?;

    info!("Excel 已生成: {}", output_path);
    Ok(())
}

/// 写入汇总信息区域
fn write_summary_section(
    ws: &Worksheet,
    start_row: u32,
    data: &SettlementData,
    header_format: &Format,
    data_format: &Format,
    amount_format: &Format,
) -> Result<u32> {
    let mut row = start_row;

    // 标题
    ws.merge_range(row, 0, row, 2, "结算单汇总信息", header_format)?;
    row += 1;

    // 合同基本信息 - 每行写入 label + value
    let info_items: Vec<(String, String)> = vec![
        ("合同编号".to_string(), data.contract_no.clone()),
        ("合同名称".to_string(), data.contract_name.clone()),
        ("作业时间".to_string(), data.work_period.clone()),
    ];

    for (label, value) in &info_items {
        ws.merge_range(row, 0, row, 1, &format!("{}：", label), data_format)?;
        ws.write_with_format(row, 2, value.as_str(), data_format)?;
        row += 1;
    }

    // 空行
    row += 1;

    // 费用信息
    let fee_items: Vec<(String, f64)> = vec![
        ("作业费用".to_string(), data.work_fee),
        ("考核金额合计".to_string(), data.total_assessment),
        ("嘉奖金额合计".to_string(), data.total_reward),
        ("当月结算费用".to_string(), data.get_settlement_amount()),
    ];

    for (label, value) in &fee_items {
        ws.merge_range(row, 0, row, 1, &format!("{}：", label), data_format)?;
        ws.write_with_format(row, 2, *value, amount_format)?;
        row += 1;
    }

    // 空行
    row += 1;

    // 区域考核概览标题
    ws.merge_range(row, 0, row, 2, "区域考核概览", header_format)?;
    row += 1;

    // 表头
    let area_headers = ["区域", "条数", "考核金额小计"];
    for (col_idx, header) in area_headers.iter().enumerate() {
        ws.write_with_format(row, col_idx as u16, *header, header_format)?;
    }
    row += 1;

    // 数据行
    for (area_name, area_data) in &data.areas {
        if area_data.records.is_empty() {
            continue;
        }
        ws.write_with_format(row, 0, area_name.as_str(), data_format)?;
        ws.write_with_format(row, 1, area_data.records.len() as f64, data_format)?;
        ws.write_with_format(row, 2, area_data.subtotal, amount_format)?;
        row += 1;
    }

    Ok(row + 1) // 空一行再返回
}

/// 写入单个区域明细
fn write_area_section(
    ws: &Worksheet,
    start_row: u32,
    area_data: &AreaData,
    header_format: &Format,
    data_format: &Format,
    amount_format: &Format,
    style: &ExcelStyle,
) -> Result<u32> {
    let mut row = start_row;

    // 区域标题行
    ws.merge_range(
        row,
        0,
        row,
        2,
        &format!("{}考核明细", area_data.name),
        header_format,
    )?;
    row += 1;

    // 表头行
    ws.set_row_height(row, style.header_row_height)?;
    let headers = ["考核\n事项", "条款", "考核\n金额"];
    for (col_idx, header) in headers.iter().enumerate() {
        ws.write_with_format(row, col_idx as u16, *header, header_format)?;
    }
    row += 1;

    // 数据行
    for record in &area_data.records {
        let desc = if record.description.is_empty() {
            ""
        } else {
            &record.description
        };
        ws.write_with_format(row, 0, desc, data_format)?;
        ws.write_with_format(row, 1, &record.clause, data_format)?;
        ws.write_with_format(row, 2, record.amount, amount_format)?;
        row += 1;
    }

    // 小计行
    ws.merge_range(row, 0, row, 1, "小计", header_format)?;
    ws.write_with_format(row, 2, area_data.subtotal, amount_format)?;
    row += 1;

    // 事业部考核金额行
    ws.merge_range(row, 0, row, 1, "事业部考核金额", header_format)?;
    ws.write_with_format(row, 2, area_data.dept_amount, amount_format)?;
    row += 1;

    Ok(row)
}