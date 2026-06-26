//! 生成格式化的 Excel 结算单明细文件，与 Python 版输出保持一致。

use std::path::Path;

use log::info;
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet, Color};

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
    summary_only: bool,
) -> Result<()> {
    info!("正在生成 Excel: {}", output_path);

    // 确保输出目录存在
    if let Some(parent) = Path::new(output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            XingDaError::ExcelWrite(format!("无法创建输出目录: {}", e))
        })?;
    }

    let mut workbook = Workbook::new();

    // ---- 细边框（与 Python openpyxl Side(style="thin") 一致） ----
    let thin_border = FormatBorder::Thin;

    // ---- 定义格式 ----

    // 居中 + 边框 + 普通字体
    let center_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(thin_border);

    // 居中 + 边框 + 加粗（用于表头/汇总标题）
    let center_bold_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(thin_border);

    // 左对齐 + 边框 + 普通字体（用于标签列）
    let left_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(thin_border);

    // 左对齐 + 自动换行 + 边框（用于考核事项描述）
    let left_wrap_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
        .set_border(thin_border);

    // 居中 + 自动换行 + 边框 + 加粗（用于表头单元格）
    let center_wrap_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
        .set_border(thin_border);

    // 居中 + 数字格式 + 边框
    let amount_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_num_format("#,##0.00")
        .set_border(thin_border);

    // 校验错误警告格式：红字 + 加粗 + 左对齐 + 自动换行 + 边框
    let validation_error_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_font_color(Color::Red)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
        .set_border(thin_border);

    // 创建工作表
    let mut worksheet = workbook.add_worksheet();

    // 设置列宽
    worksheet.set_column_width(0, style.col_width_a)?;
    worksheet.set_column_width(1, style.col_width_b)?;
    worksheet.set_column_width(2, style.col_width_c)?;

    let mut current_row: u32 = 0;

    // --- 1. 汇总信息区域 ---
    if include_summary {
        current_row = write_summary_section(
            &mut worksheet,
            current_row,
            data,
            &center_bold_format,
            &left_format,
            &center_format,
            &amount_format,
        )?;
    }

    // --- 2. 区域明细 ---
    if !summary_only {
        for (idx, area_name) in area_order.iter().enumerate() {
            if let Some(area_data) = data.areas.get(area_name) {
                if area_data.records.is_empty() {
                    continue;
                }

                if idx > 0 {
                    current_row += 1;
                }

                current_row = write_area_section(
                    &mut worksheet,
                    current_row,
                    area_data,
                    &center_format,
                    &left_wrap_format,
                    &center_wrap_format,
                    &amount_format,
                    &center_bold_format,
                    style,
                )?;
            }
        }
    }

    // --- 3. 校验失败警告行 ---
    if !data.amount_match {
        current_row += 1;
        let warning_text = format!(
            "⚠ 金额校验失败：PDF 声明合计 ¥{:.2}，程序提取合计 ¥{:.2}，偏差 {:.2}%",
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
            &validation_error_format,
        )?;
    }

    workbook.save(output_path)?;

    info!("Excel 已生成: {}", output_path);
    Ok(())
}

/// 写入汇总信息区域
fn write_summary_section(
    ws: &mut Worksheet,
    start_row: u32,
    data: &SettlementData,
    center_bold: &Format,
    left_format: &Format,
    center_format: &Format,
    amount_format: &Format,
) -> Result<u32> {
    let mut row = start_row;

    // 标题行（加粗 + 居中 + 边框）
    ws.merge_range(row, 0, row, 2, "结算单汇总信息", center_bold)?;
    row += 1;

    // 合同基本信息（标签左对齐，值居中）
    let info_items: Vec<(String, String)> = vec![
        ("合同编号".to_string(), data.contract_no.clone()),
        ("合同名称".to_string(), data.contract_name.clone()),
        ("作业时间".to_string(), data.work_period.clone()),
    ];

    for (label, value) in &info_items {
        ws.merge_range(row, 0, row, 1, &format!("{}：", label), left_format)?;
        ws.write_with_format(row, 2, value.as_str(), center_format)?;
        row += 1;
    }

    // 空行
    row += 1;

    // 费用项目
    let fee_items: Vec<(String, f64)> = vec![
        ("作业费用".to_string(), data.work_fee),
        ("考核金额合计".to_string(), data.total_assessment),
        ("嘉奖金额合计".to_string(), data.total_reward),
        ("当月结算费用".to_string(), data.settlement_amount_or_computed()),
    ];

    for (label, value) in &fee_items {
        ws.merge_range(row, 0, row, 1, &format!("{}：", label), left_format)?;
        ws.write_with_format(row, 2, *value, amount_format)?;
        row += 1;
    }

    row += 1;

    // 区域考核概览标题
    ws.merge_range(row, 0, row, 2, "区域考核概览", center_bold)?;
    row += 1;

    // 区域概览表头（居中 + 加粗 + 边框）
    let area_headers = ["区域", "条数", "考核金额小计"];
    for (col_idx, header) in area_headers.iter().enumerate() {
        ws.write_with_format(row, col_idx as u16, *header, center_bold)?;
    }
    row += 1;

    // 区域概览数据行
    for (area_name, area_data) in &data.areas {
        if area_data.records.is_empty() {
            continue;
        }
        ws.write_with_format(row, 0, area_name.as_str(), center_format)?;
        ws.write_with_format(row, 1, area_data.records.len() as f64, center_format)?;
        ws.write_with_format(row, 2, area_data.subtotal, amount_format)?;
        row += 1;
    }

    Ok(row + 1)
}

/// 写入单个区域明细
fn write_area_section(
    ws: &mut Worksheet,
    start_row: u32,
    area_data: &AreaData,
    center_format: &Format,
    left_wrap_format: &Format,
    center_wrap_format: &Format,
    amount_format: &Format,
    center_bold: &Format,
    style: &ExcelStyle,
) -> Result<u32> {
    let mut row = start_row;

    // 区域标题行（普通字体 + 居中 + 边框，与 Python 版一致：不用加粗）
    ws.merge_range(
        row,
        0,
        row,
        2,
        &format!("{}考核明细", area_data.name),
        center_format,
    )?;
    row += 1;

    // 表头行（加粗 + 居中 + 自动换行 + 边框）
    ws.set_row_height(row, style.header_row_height)?;
    let headers = ["考核\n事项", "条款", "考核\n金额"];
    for (col_idx, header) in headers.iter().enumerate() {
        ws.write_with_format(row, col_idx as u16, *header, center_wrap_format)?;
    }
    row += 1;

    // 数据行
    for record in &area_data.records {
        // 空描述时写入空串，避免显示错误
        let desc = if record.description.is_empty() {
            ""
        } else {
            &record.description
        };
        // 考核事项：左对齐 + 自动换行 + 边框
        ws.write_with_format(row, 0, desc, left_wrap_format)?;
        // 条款：居中 + 边框
        ws.write_with_format(row, 1, &record.clause, center_format)?;
        // 金额：居中 + 数字格式 + 边框
        ws.write_with_format(row, 2, record.amount, amount_format)?;
        row += 1;
    }

    // 小计行（加粗 + 居中 + 边框）
    ws.merge_range(row, 0, row, 1, "小计", center_bold)?;
    ws.write_with_format(row, 2, area_data.subtotal, amount_format)?;
    row += 1;

    // 事业部考核金额行
    ws.merge_range(row, 0, row, 1, "事业部考核金额", center_bold)?;
    ws.write_with_format(row, 2, area_data.dept_amount, amount_format)?;
    row += 1;

    Ok(row)
}