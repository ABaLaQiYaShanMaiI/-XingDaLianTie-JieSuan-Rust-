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

    if let Some(parent) = Path::new(output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            XingDaError::ExcelWrite(format!("无法创建输出目录 {}: {}", parent.display(), e))
        })?;
    }

    let mut workbook = Workbook::new();
    let thin_border = FormatBorder::Thin;

    let center_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(thin_border);

    let center_bold_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(thin_border);

    let left_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(thin_border);

    let left_wrap_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
        .set_border(thin_border);

    let center_wrap_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
        .set_border(thin_border);

    let amount_format = Format::new()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_num_format("#,##0.00")
        .set_border(thin_border);

    let validation_error_format = Format::new()
        .set_bold()
        .set_font_size(style.font_size)
        .set_font_name(&style.font_name)
        .set_font_color(Color::Red)
        .set_align(FormatAlign::Left)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
        .set_border(thin_border);

    let mut worksheet = workbook.add_worksheet();

    worksheet.set_column_width(0, style.col_width_a)?;
    worksheet.set_column_width(1, style.col_width_b)?;
    worksheet.set_column_width(2, style.col_width_c)?;

    let mut current_row: u32 = 0;

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

    if !data.amount_match {
        current_row += 1;
        let warning_text = format!(
            "⚠ 金额校验失败：PDF 声明合计 ¥{:.2}，程序提取合计 ¥{:.2}，偏差 {:.2}%",
            data.pdf_stated_total.unwrap_or(0.0),
            data.total_assessment,
            data.amount_deviation_pct * 100.0
        );
        worksheet.merge_range(current_row, 0, current_row, 2, &warning_text, &validation_error_format)?;
    }

    workbook.save(output_path)?;
    info!("Excel 已生成: {}", output_path);
    Ok(())
}

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
    ws.merge_range(row, 0, row, 2, "结算单汇总信息", center_bold)?;
    row += 1;
    for (label, value) in &[
        ("合同编号", data.contract_no.clone()),
        ("合同名称", data.contract_name.clone()),
        ("作业时间", data.work_period.clone()),
    ] {
        ws.merge_range(row, 0, row, 1, &format!("{}：", label), left_format)?;
        ws.write_with_format(row, 2, value.as_str(), center_format)?;
        row += 1;
    }
    row += 1;
    for (label, value) in &[
        ("作业费用", data.work_fee),
        ("考核金额合计", data.total_assessment),
        ("嘉奖金额合计", data.total_reward),
        ("当月结算费用", data.settlement_amount_or_computed()),
    ] {
        ws.merge_range(row, 0, row, 1, &format!("{}：", label), left_format)?;
        ws.write_with_format(row, 2, *value, amount_format)?;
        row += 1;
    }
    row += 1;
    ws.merge_range(row, 0, row, 2, "区域考核概览", center_bold)?;
    row += 1;
    for (ci, h) in ["区域", "条数", "考核金额小计"].iter().enumerate() {
        ws.write_with_format(row, ci as u16, *h, center_bold)?;
    }
    row += 1;
    for (an, ad) in &data.areas {
        if ad.records.is_empty() {
            continue;
        }
        ws.write_with_format(row, 0, an.as_str(), center_format)?;
        ws.write_with_format(row, 1, ad.records.len() as f64, center_format)?;
        ws.write_with_format(row, 2, ad.subtotal, amount_format)?;
        row += 1;
    }
    Ok(row + 1)
}

fn write_area_section(
    ws: &mut Worksheet,
    start_row: u32,
    area_data: &AreaData,
    center_format: &Format,
    left_wrap_format: &Format,
    center_wrap_format: &Format,
    amount_format: &Format,
    center_bold_format: &Format,
    _style: &ExcelStyle,
) -> Result<u32> {
    let mut row = start_row;
    ws.merge_range(row, 0, row, 2, &area_data.name, center_bold_format)?;
    row += 1;
    for (ci, h) in ["考核事项", "金额（元）", "备注"].iter().enumerate() {
        ws.write_with_format(row, ci as u16, *h, center_wrap_format)?;
    }
    row += 1;
    for record in &area_data.records {
        ws.write_with_format(row, 0, &record.description, left_wrap_format)?;
        ws.write_with_format(row, 1, record.amount, amount_format)?;
        ws.write_with_format(row, 2, &record.remark, center_format)?;
        row += 1;
    }
    ws.merge_range(row, 0, row, 1, "小计", center_bold_format)?;
    ws.write_with_format(row, 2, area_data.subtotal, amount_format)?;
    row += 1;
    if !area_data.amount_match {
        let deviation = area_data.amount_deviation_pct * 100.0;
        let warn = format!(
            "⚠ 区域闭合校验失败：PDF声明 {:.2}，提取合计 {:.2}，偏差 {:.2}%",
            area_data.pdf_stated_total.unwrap_or(0.0),
            area_data.computed_total,
            deviation
        );
        ws.merge_range(row, 0, row, 2, &warn, left_wrap_format)?;
        row += 1;
    }
    Ok(row + 1)
}