//! 图形化界面 (egui/eframe)：文件选择、PDF 处理、实时日志、拖拽、环境检测。


use std::sync::mpsc;
use std::thread;

use eframe::egui;
use log::{info, warn};

use crate::cli::process_pdf_core;
use crate::config::StylePreset;
use crate::excel_writer::generate_excel;
use crate::config::ParserConfig;
use crate::validator::generate_validation_summary;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 日志消息类型
#[derive(Debug, Clone)]
enum LogMessage {
    Info(String),
    Success(String),
    Warning(String),
    Error(String),
}

/// 外部工具检测状态
#[derive(Debug, Clone, PartialEq)]
enum ToolStatus {
    /// 已检测到
    Found(String),
    /// 未检测到
    NotFound,
    /// 未检测（初始状态）
    Unknown,
}

impl ToolStatus {
    fn is_found(&self) -> bool {
        matches!(self, ToolStatus::Found(_))
    }
}

/// GUI 应用状态
struct XingDaApp {
    // 文件路径
    pdf_path: String,
    output_dir: String,
    rules_path: String,

    // 选项
    validate_only: bool,
    dump_text: bool,
    no_summary: bool,
    enable_ocr: bool,
    summary_only: bool,
    no_merge: bool,
    style_preset: Option<StylePreset>,
    log_file_path: String,

    // 处理状态
    processing: bool,
    log_messages: Vec<LogMessage>,

    // 后台处理通信
    log_receiver: Option<mpsc::Receiver<LogMessage>>,
    result_receiver: Option<mpsc::Receiver<Result<String, String>>>,

    // 输出结果
    last_output: Option<String>,

    // 环境检测状态
    ghostscript_status: ToolStatus,
    tesseract_status: ToolStatus,
    env_check_done: bool,

    // 拖拽提示
    drag_hover: bool,
}

impl Default for XingDaApp {
    fn default() -> Self {
        Self {
            pdf_path: String::new(),
            output_dir: String::from("./output"),
            rules_path: String::new(),
            validate_only: false,
            dump_text: false,
            no_summary: false,
            enable_ocr: false,
            summary_only: false,
            no_merge: false,
            style_preset: None,
            log_file_path: String::new(),
            processing: false,
            log_messages: Vec::new(),
            log_receiver: None,
            result_receiver: None,
            last_output: None,
            ghostscript_status: ToolStatus::Unknown,
            tesseract_status: ToolStatus::Unknown,
            env_check_done: false,
            drag_hover: false,
        }
    }
}

impl eframe::App for XingDaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ---- 首次运行环境检测 ----
        if !self.env_check_done {
            self.run_env_check();
            self.env_check_done = true;
        }

        // ---- 处理拖拽文件 ----
        self.handle_dropped_files(ctx);

        // 轮询后台处理结果
        self.poll_results();

        // ---- 拖拽悬停高亮 ----
        if self.drag_hover {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "📂 释放以加载 PDF - 兴达结算单工具 v{}",
                VERSION
            )));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "兴达炼铁保产事业部 结算单明细工具 v{}",
                VERSION
            )));
        }

        // ---- 顶部面板 ----
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.heading(format!("兴达炼铁保产事业部 · 结算单明细工具 v{}", VERSION));
            ui.label("PDF 结算单 → 自动提取考核明细 → 生成 Excel");
            if !self.processing {
                if self.drag_hover {
                    ui.colored_label(
                        egui::Color32::from_rgb(0, 200, 100),
                        "📂 释放 PDF 文件到此处！",
                    );
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(0, 120, 212),
                        "📂 拖拽 PDF 文件到此处，或使用下方文件浏览器",
                    );
                }
            }
        });

        // ---- 中央面板 ----
        egui::CentralPanel::default().show(ctx, |ui| {
            // --- 环境检测状态 ---
            ui.collapsing("🔧 环境检测", |ui| {
                self.render_env_status(ui);
            });

            ui.separator();

            // 文件选择区域
            ui.collapsing("📄 文件选择", |ui| {
                // PDF 文件
                ui.horizontal(|ui| {
                    ui.label("PDF 文件：");
                    ui.text_edit_singleline(&mut self.pdf_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PDF 文件", &["pdf"])
                            .pick_file()
                        {
                            self.pdf_path = path.to_string_lossy().to_string();
                        }
                    }
                });

                // 输出目录
                ui.horizontal(|ui| {
                    ui.label("输出目录：");
                    ui.text_edit_singleline(&mut self.output_dir);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.output_dir = path.to_string_lossy().to_string();
                        }
                    }
                });

                // 分类规则
                ui.horizontal(|ui| {
                    ui.label("分类规则：");
                    ui.text_edit_singleline(&mut self.rules_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("YAML", &["yaml", "yml"])
                            .pick_file()
                        {
                            self.rules_path = path.to_string_lossy().to_string();
                        }
                    }
                    ui.colored_label(
                        egui::Color32::GRAY,
                        "（可选，默认使用 classify_rules.yaml）",
                    );
                });
            });

            // 基本选项
            ui.collapsing("⚙ 选项", |ui| {
                ui.checkbox(
                    &mut self.validate_only,
                    "仅校验不生成 Excel（--validate-only）",
                );
                ui.checkbox(&mut self.dump_text, "同时导出原始文本（--dump-text）");
                ui.checkbox(
                    &mut self.no_summary,
                    "不生成汇总信息区域（--no-summary）",
                );
                ui.checkbox(
                    &mut self.summary_only,
                    "仅生成汇总 sheet，跳过区域明细（--summary-only）",
                );
                ui.checkbox(
                    &mut self.enable_ocr,
                    "启用 OCR（PDF 无文本层时使用 Tesseract+Ghostscript）",
                );
                // 如果 OCR 已启用但工具未安装，给出警告
                if self.enable_ocr
                    && (!self.ghostscript_status.is_found()
                        || !self.tesseract_status.is_found())
                {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 170, 60),
                        "⚠ 注意：OCR 工具未完全安装，请检查「环境检测」面板",
                    );
                }

                ui.separator();

                // Excel 样式预设
                ui.horizontal(|ui| {
                    ui.label("Excel 样式：");
                    ui.selectable_value(
                        &mut self.style_preset,
                        None,
                        "默认",
                    );
                    ui.selectable_value(
                        &mut self.style_preset,
                        Some(StylePreset::Compact),
                        "紧凑",
                    );
                    ui.selectable_value(
                        &mut self.style_preset,
                        Some(StylePreset::Wide),
                        "宽松",
                    );
                });
            });

            // 高级选项
            ui.collapsing("🔧 高级选项", |ui| {
                ui.checkbox(
                    &mut self.no_merge,
                    "禁用多行合并（调试用，--no-merge）",
                );

                ui.horizontal(|ui| {
                    ui.label("日志文件：");
                    ui.text_edit_singleline(&mut self.log_file_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("日志", &["log", "txt"])
                            .save_file()
                        {
                            self.log_file_path = path.to_string_lossy().to_string();
                        }
                    }
                });
                if !self.log_file_path.is_empty() {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        "日志将输出到文件（轮转，每个文件最大 5MB）",
                    );
                }
            });

            ui.separator();

            // 操作按钮
            ui.horizontal(|ui| {
                let can_process = !self.pdf_path.is_empty() && !self.processing;
                if ui
                    .add_enabled(
                        can_process,
                        egui::Button::new("▶  开始处理")
                            .min_size(egui::vec2(120.0, 30.0)),
                    )
                    .clicked()
                {
                    self.start_processing();
                }

                if ui.button("📁 打开输出目录").clicked() {
                    self.open_output_dir();
                }

                if ui.button("🔄 重新检测环境").clicked() {
                    self.run_env_check();
                }

                if ui.button("清空日志").clicked() {
                    self.log_messages.clear();
                }
            });

            // 处理状态
            if self.processing {
                ui.add(egui::Spinner::new());
                ui.label("处理中...");
            }

            ui.separator();

            // 日志区域
            ui.collapsing("📋 处理日志", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for msg in &self.log_messages {
                            match msg {
                                LogMessage::Info(text) => {
                                    ui.colored_label(egui::Color32::WHITE, text);
                                }
                                LogMessage::Success(text) => {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(106, 153, 85),
                                        text,
                                    );
                                }
                                LogMessage::Warning(text) => {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(206, 145, 120),
                                        text,
                                    );
                                }
                                LogMessage::Error(text) => {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(244, 71, 71),
                                        text,
                                    );
                                }
                            }
                        }
                    });
            });
        });

        // ---- 底部状态栏 ----
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            // 工具状态快速指示器
            ui.horizontal(|ui| {
                self.render_tool_pill(ui, "GS", &self.ghostscript_status);
                ui.separator();
                self.render_tool_pill(ui, "Tesseract", &self.tesseract_status);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(ref output) = self.last_output {
                        ui.colored_label(
                            egui::Color32::from_rgb(106, 153, 85),
                            format!("✅ 输出: {}", output),
                        );
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "就绪");
                    }
                });
            });
        });
    }
}

impl XingDaApp {
    /// 运行环境检测
    fn run_env_check(&mut self) {
        self.ghostscript_status = detect_ghostscript();
        self.tesseract_status = detect_tesseract();
        self.env_check_done = true;
    }

    /// 处理拖拽文件
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| {
            let files = i.raw.dropped_files.clone();
            let hovering = i.raw.hovered_files.len() > 0;
            (files, hovering)
        });

        self.drag_hover = dropped.1 && !self.processing;

        if let Some(file) = dropped.0.first() {
            if let Some(path) = &file.path {
                let path_str = path.to_string_lossy().to_lowercase();
                if path_str.ends_with(".pdf") {
                    self.pdf_path = path.to_string_lossy().to_string();
                    info!("拖拽加载 PDF: {}", self.pdf_path);
                }
            }
        }
    }

    /// 渲染环境检测面板
    fn render_env_status(&self, ui: &mut egui::Ui) {
        ui.label("外部工具检测（用于 OCR 功能）：");
        ui.add_space(4.0);

        // Ghostscript
        ui.horizontal(|ui| {
            match &self.ghostscript_status {
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
            match &self.tesseract_status {
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
        if !self.ghostscript_status.is_found() || !self.tesseract_status.is_found() {
            ui.colored_label(
                egui::Color32::from_rgb(255, 170, 60),
                "ℹ 未安装的工具不影响普通 PDF 处理，仅 OCR 扫描件时需要。",
            );
        }
    }

    /// 渲染底部状态栏的工具状态指示器
    fn render_tool_pill(&self, ui: &mut egui::Ui, label: &str, status: &ToolStatus) {
        let (color, symbol) = match status {
            ToolStatus::Found(_) => (egui::Color32::from_rgb(106, 153, 85), "●"),
            ToolStatus::NotFound => (egui::Color32::from_rgb(244, 71, 71), "●"),
            ToolStatus::Unknown => (egui::Color32::GRAY, "○"),
        };
        ui.colored_label(color, format!("{} {}", symbol, label));
    }

    /// 开始后台处理
    fn start_processing(&mut self) {
        self.processing = true;
        self.log_messages.clear();
        self.last_output = None;

        let pdf_path = self.pdf_path.clone();
        let output_dir = self.output_dir.clone();
        let rules_path = if self.rules_path.is_empty() {
            None
        } else {
            Some(self.rules_path.clone())
        };
        let validate_only = self.validate_only;
        let dump_text = self.dump_text;
        let include_summary = !self.no_summary;
        let enable_ocr = self.enable_ocr;
        let summary_only = self.summary_only;
        let no_merge = self.no_merge;
        let style_preset = self.style_preset;
        let log_file_path = if self.log_file_path.is_empty() {
            None
        } else {
            Some(self.log_file_path.clone())
        };

        let (log_tx, log_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();

        self.log_receiver = Some(log_rx);
        self.result_receiver = Some(result_rx);

        thread::spawn(move || {
            let send_log = |msg: LogMessage| {
                let _ = log_tx.send(msg);
            };

            send_log(LogMessage::Info(format!("🚀 开始处理: {}", pdf_path)));

            let result = process_in_thread(
                &pdf_path,
                &output_dir,
                rules_path.as_deref(),
                validate_only,
                dump_text,
                include_summary,
                enable_ocr,
                summary_only,
                no_merge,
                style_preset,
                log_file_path.as_deref(),
                &send_log,
            );

            match result {
                Ok(output) => {
                    if output.is_empty() {
                        send_log(LogMessage::Warning(
                            "⚠ 处理完成但未生成文件".to_string(),
                        ));
                        let _ = result_tx.send(Err("无文件生成".to_string()));
                    } else {
                        send_log(LogMessage::Success(format!(
                            "✅ 完成！输出文件: {}",
                            output
                        )));
                        let _ = result_tx.send(Ok(output));
                    }
                }
                Err(e) => {
                    send_log(LogMessage::Error(format!("❌ 错误: {}", e)));
                    let _ = result_tx.send(Err(e));
                }
            }
        });
    }

    /// 轮询后台处理结果
    fn poll_results(&mut self) {
        if let Some(ref rx) = self.log_receiver {
            while let Ok(msg) = rx.try_recv() {
                self.log_messages.push(msg);
            }
        }

        if let Some(ref rx) = self.result_receiver {
            if let Ok(result) = rx.try_recv() {
                self.processing = false;
                match result {
                    Ok(output_path) => {
                        self.last_output = Some(output_path);
                    }
                    Err(e) => {
                        self.last_output = Some(format!("处理失败: {}", e));
                    }
                }
            }
        }
    }

    /// 打开输出目录（跨平台）
    fn open_output_dir(&self) {
        let path = if self.output_dir.is_empty() {
            "./output".to_string()
        } else {
            self.output_dir.clone()
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
}

// ============================================================
// 环境检测函数（跨平台）
// ============================================================

/// 检测 Ghostscript 安装（委托 ocr.rs 共享逻辑）
fn detect_ghostscript() -> ToolStatus {
    match crate::ocr::find_ghostscript() {
        Some(path) => ToolStatus::Found(path.to_string_lossy().into_owned()),
        None => ToolStatus::NotFound,
    }
}

/// 检测 Tesseract 安装（委托 ocr.rs 共享逻辑）
fn detect_tesseract() -> ToolStatus {
    match crate::ocr::find_tesseract() {
        Some(path) => ToolStatus::Found(path.to_string_lossy().into_owned()),
        None => ToolStatus::NotFound,
    }
}

// ============================================================
// 后台处理
// ============================================================

/// 在后台线程中执行处理（复用 CLI 核心流程）
fn process_in_thread(
    pdf_path: &str,
    output_dir: &str,
    rules_path: Option<&str>,
    validate_only: bool,
    dump_text: bool,
    include_summary: bool,
    enable_ocr: bool,
    summary_only: bool,
    no_merge: bool,
    style_preset: Option<StylePreset>,
    log_file_path: Option<&str>,
    send_log: &dyn Fn(LogMessage),
) -> Result<String, String> {
    send_log(LogMessage::Info("正在解析 PDF...".to_string()));

    // 使用默认 ParserConfig（GUI 暂不支持自定义 OCR DPI / 语言 / PSM 参数）
    let parser_config = ParserConfig::default();

    // --- 核心处理：解析 + 分类 + 校验（复用 CLI 共享函数） ---
    let (mut data, rules, excel_style, is_valid) = process_pdf_core(
        pdf_path,
        rules_path,
        enable_ocr,
        no_merge,
        &parser_config,
        style_preset,
    )
    .map_err(|e| format!("处理失败: {}", e))?;

    // 导出原始文本
    if dump_text {
        let pdf_path_obj = std::path::Path::new(pdf_path);
        let pdf_stem = pdf_path_obj
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let txt_path = std::path::Path::new(output_dir)
            .join(format!("{}.txt", pdf_stem));
        let header = format!(
            "=== PDF 原始文本导出 ===\n文件: {}\n提取字符数: {}\n=========================\n\n",
            pdf_path,
            data.raw_text.len()
        );
        let content = header + &data.raw_text;
        if let Err(e) = std::fs::write(&txt_path, &content) {
            send_log(LogMessage::Warning(format!("导出原始文本失败: {}", e)));
        } else {
            send_log(LogMessage::Info(format!("原始文本已导出: {} ({} 字符)", txt_path.display(), data.raw_text.len())));
        }
    }

    send_log(LogMessage::Info(format!(
        "提取 {} 条考核记录",
        data.all_records.len()
    )));

    let summary = generate_validation_summary(&data);

    for line in summary.lines() {
        send_log(LogMessage::Info(line.to_string()));
    }

    if validate_only {
        if is_valid {
            send_log(LogMessage::Success("校验通过".to_string()));
        } else {
            send_log(LogMessage::Error("校验失败".to_string()));
        }
        return Ok(String::new());
    }

    // 生成 Excel
    let pdf_path_obj = std::path::Path::new(pdf_path);
    let excel_name = format!(
        "{}明细.xlsx",
        pdf_path_obj
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
    );

    let output_path = std::path::Path::new(output_dir).join(&excel_name);
    let output_path_str = output_path
        .to_str()
        .ok_or_else(|| "输出路径包含非法字符".to_string())?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    generate_excel(
        &data,
        output_path_str,
        &rules.area_order,
        &excel_style,
        include_summary,
        summary_only,
    )
    .map_err(|e| format!("Excel生成失败: {}", e))?;

    // 日志文件（如果指定）
    if let Some(log_path) = log_file_path {
        let log_content = format!(
            "=== 处理日志 ===\n文件: {}\n{}\n",
            pdf_path,
            summary
        );
        if let Err(e) = std::fs::write(log_path, &log_content) {
            send_log(LogMessage::Warning(format!("日志文件写入失败: {}", e)));
        } else {
            send_log(LogMessage::Info(format!("日志已保存: {}", log_path)));
        }
    }

    Ok(output_path_str.to_string())
}

// ============================================================
// GUI 启动
// ============================================================

/// 启动 GUI
pub fn launch_gui() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("兴达炼铁保产事业部 结算单明细工具")
            .with_inner_size([680.0, 700.0])
            .with_min_inner_size([600.0, 500.0]),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "XingDa JieSuan",
        options,
        Box::new(|cc| {
            setup_chinese_fonts(&cc.egui_ctx);
            Ok(Box::new(XingDaApp::default()))
        }),
    ) {
        eprintln!("GUI 启动失败: {}", e);
    }
}

/// 从系统字体目录加载中文字体（跨平台）
///
/// 按优先级依次尝试系统常见中文字体路径，首个存在的字体即被加载并设置为默认。
fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let font_paths: Vec<&str> = {
        #[cfg(target_os = "windows")]
        {
            vec![
                r"C:\Windows\Fonts\msyh.ttc",
                r"C:\Windows\Fonts\msyh.ttf",
                r"C:\Windows\Fonts\simsun.ttc",
                r"C:\Windows\Fonts\simhei.ttf",
                r"C:\Windows\Fonts\simfang.ttf",
            ]
        }
        #[cfg(target_os = "macos")]
        {
            vec![
                "/Library/Fonts/STHeiti Light.ttc",
                "/System/Library/Fonts/PingFang.ttc",
                "/Library/Fonts/Noto Sans CJK JP/NotoSansCJKjp-Regular.otf",
                "/Library/Fonts/SimHei.ttf",
                "/System/Library/Fonts/STHeiti Medium.ttc",
            ]
        }
        #[cfg(target_os = "linux")]
        {
            vec![
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            ]
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )))]
        {
            vec![]
        }
    };

    let font_name = "chinese_font";
    for path in &font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            info!("已加载中文字体: {}", path);

            fonts.font_data.insert(
                font_name.to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes.to_vec())),
            );

            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, font_name.to_owned());

            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, font_name.to_owned());

            ctx.set_fonts(fonts);
            return;
        }
    }

    warn!("未找到系统中文字体，使用默认字体");
    ctx.set_fonts(fonts);
}