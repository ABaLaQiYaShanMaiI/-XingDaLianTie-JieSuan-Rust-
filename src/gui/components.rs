//! GUI 组件渲染与环境检测辅助函数

use log::{info, warn};

use crate::config::StylePreset;
use crate::ocr;

/// 日志消息类型
#[derive(Debug, Clone)]
pub enum LogMessage {
    Info(String),
    Success(String),
    Warning(String),
    Error(String),
}

/// 外部工具检测状态
#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    /// 已检测到
    Found(String),
    /// 未检测到
    NotFound,
    /// 未检测（初始状态）
    Unknown,
}

impl ToolStatus {
    pub fn is_found(&self) -> bool {
        matches!(self, ToolStatus::Found(_))
    }
}

/// 检测 Ghostscript 安装
pub fn detect_ghostscript() -> ToolStatus {
    match ocr::find_ghostscript() {
        Some(path) => ToolStatus::Found(path.to_string_lossy().into_owned()),
        None => ToolStatus::NotFound,
    }
}

/// 检测 Tesseract 安装
pub fn detect_tesseract() -> ToolStatus {
    match ocr::find_tesseract() {
        Some(path) => ToolStatus::Found(path.to_string_lossy().into_owned()),
        None => ToolStatus::NotFound,
    }
}

/// 渲染环境检测面板
pub fn render_env_status(ui: &mut egui::Ui, gs: &ToolStatus, ts: &ToolStatus) {
    ui.label("外部工具检测（用于 OCR 功能）：");
    ui.add_space(4.0);

    // Ghostscript
    ui.horizontal(|ui| {
        match gs {
            ToolStatus::Found(path) => {
                ui.colored_label(
                    egui::Color32::from_rgb(106, 153, 85),
                    "✅",
                );
                ui.label(format!("Ghostscript: {}", path));
            }
            ToolStatus::NotFound => {
                ui.colored_label(
                    egui::Color32::from_rgb(244, 71, 71),
                    "❌",
                );
                ui.label("Ghostscript: 未安装");
                ui.hyperlink_to(
                    "下载",
                    "https://ghostscript.com/releases/gsdnld.html",
                );
            }
            ToolStatus::Unknown => {
                ui.colored_label(
                    egui::Color32::GRAY,
                    "⏳",
                );
                ui.label("Ghostscript: 检测中...");
            }
        }
    });

    // Tesseract
    ui.horizontal(|ui| {
        match ts {
            ToolStatus::Found(path) => {
                ui.colored_label(
                    egui::Color32::from_rgb(106, 153, 85),
                    "✅",
                );
                ui.label(format!("Tesseract: {}", path));
            }
            ToolStatus::NotFound => {
                ui.colored_label(
                    egui::Color32::from_rgb(244, 71, 71),
                    "❌",
                );
                ui.label("Tesseract: 未安装（需勾选 chi_sim 中文语言包）");
                ui.hyperlink_to(
                    "下载",
                    "https://github.com/UB-Mannheim/tesseract/wiki",
                );
            }
            ToolStatus::Unknown => {
                ui.colored_label(
                    egui::Color32::GRAY,
                    "⏳",
                );
                ui.label("Tesseract: 检测中...");
            }
        }
    });

    ui.add_space(4.0);
    if !gs.is_found() || !ts.is_found() {
        ui.colored_label(
            egui::Color32::from_rgb(255, 170, 60),
            "ℹ 未安装的工具不影响普通 PDF 处理，仅 OCR 扫描件时需要。",
        );
    }
}

/// 渲染底部状态栏的工具状态指示器
pub fn render_tool_pill(ui: &mut egui::Ui, label: &str, status: &ToolStatus) {
    let (color, symbol) = match status {
        ToolStatus::Found(_) => (egui::Color32::from_rgb(106, 153, 85), "●"),
        ToolStatus::NotFound => (egui::Color32::from_rgb(244, 71, 71), "●"),
        ToolStatus::Unknown => (egui::Color32::GRAY, "○"),
    };
    ui.colored_label(color, format!("{} {}", symbol, label));
}

/// 渲染 Excel 样式选择器
pub fn render_style_selector(ui: &mut egui::Ui, current: &mut Option<StylePreset>) {
    ui.horizontal(|ui| {
        ui.label("Excel 样式：");
        ui.selectable_value(current, None, "默认");
        ui.selectable_value(current, Some(StylePreset::Compact), "紧凑");
        ui.selectable_value(current, Some(StylePreset::Wide), "宽松");
    });
}

/// 打开输出目录（跨平台）
pub fn open_output_dir(dir: &str) {
    let path = if dir.is_empty() {
        "./output".to_string()
    } else {
        dir.to_string()
    };

    let result = {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(&path)
                .spawn()
                .map(|_| ())
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&path)
                .spawn()
                .map(|_| ())
        }
        #[cfg(target_os = "linux")]
        {
            let managers = ["xdg-open", "nautilus", "dolphin", "pcmanfm"];
            let mut result = Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No file manager found",
            ));
            for manager in &managers {
                if std::process::Command::new(manager)
                    .arg(&path)
                    .spawn()
                    .is_ok()
                {
                    result = Ok(());
                    break;
                }
            }
            result.map(|_| ())
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            open::that(&path)
        }
    };

    if let Err(e) = result {
        warn!("无法打开目录: {}", e);
        info!("请手动打开: {}", path);
    }
}