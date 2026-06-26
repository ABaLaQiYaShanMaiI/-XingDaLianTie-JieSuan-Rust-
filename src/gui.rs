//! GUI 模块
//! =======
//! 提供图形化用户界面 (egui/eframe)。
//! 支持: 文件选择、PDF 处理、实时日志显示、文件拖拽、环境检测。

use std::sync::mpsc;
use std::thread;

use eframe::egui;
use log::{info, warn};

use crate::classifier::classify_records;
use crate::config::{load_rules, load_excel_style, ParserConfig};
use crate::excel_writer::generate_excel;
use crate::parser::parse_pdf;
use crate::validator::{generate_validation_summary, validate_amounts};

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

            // 选项
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

    /// 打开输出目录
    fn open_output_dir(&self) {
        let path = if self.output_dir.is_empty() {
            "./output"
        } else {
            &self.output_dir
        };

        if let Err(e) = open::that(path) {
            warn!("无法打开目录: {}", e);
        }
    }
}

// ============================================================
// 环境检测函数
// ============================================================

/// 检测 Ghostscript 安装
fn detect_ghostscript() -> ToolStatus {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // 先尝试 PATH
        if let Ok(output) = Command::new("where").arg("gswin64c.exe").output() {
            if output.status.success() {
                if let Some(line) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                {
                    return ToolStatus::Found(line.to_string());
                }
            }
        }
        // 再尝试常见安装路径
        let candidates = [
            r"C:\Program Files\gs\gs10.05.0\bin\gswin64c.exe", // latest stable
        ];
        // 扫描 Program Files\gs 目录
        use std::path::Path;
        let base_dirs = [
            r"C:\Program Files\gs",
            r"C:\Program Files (x86)\gs",
        ];
        for base in &base_dirs {
            let base_path = Path::new(base);
            if base_path.exists() {
                if let Ok(entries) = std::fs::read_dir(base_path) {
                    for entry in entries.flatten() {
                        let dir = entry.path();
                        if dir.is_dir() {
                            let gs = dir.join("bin").join("gswin64c.exe");
                            if gs.exists() {
                                return ToolStatus::Found(
                                    gs.to_string_lossy().to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }
        for p in &candidates {
            if Path::new(p).exists() {
                return ToolStatus::Found(p.to_string());
            }
        }
        ToolStatus::NotFound
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = std::process::Command::new("which")
            .arg("gs")
            .output()
        {
            if output.status.success() {
                if let Some(line) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                {
                    return ToolStatus::Found(line.to_string());
                }
            }
        }
        ToolStatus::NotFound
    }
}

/// 检测 Tesseract 安装
fn detect_tesseract() -> ToolStatus {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("where").arg("tesseract.exe").output() {
            if output.status.success() {
                if let Some(line) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                {
                    return ToolStatus::Found(line.to_string());
                }
            }
        }
        use std::path::Path;
        let candidates = [
            r"C:\Program Files\Tesseract-OCR\tesseract.exe",
            r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
        ];
        for p in &candidates {
            if Path::new(p).exists() {
                return ToolStatus::Found(p.to_string());
            }
        }
        ToolStatus::NotFound
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = std::process::Command::new("which")
            .arg("tesseract")
            .output()
        {
            if output.status.success() {
                if let Some(line) = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                {
                    return ToolStatus::Found(line.to_string());
                }
            }
        }
        ToolStatus::NotFound
    }
}

// ============================================================
// 后台处理
// ============================================================

/// 在后台线程中执行处理
fn process_in_thread(
    pdf_path: &str,
    output_dir: &str,
    rules_path: Option<&str>,
    validate_only: bool,
    dump_text: bool,
    include_summary: bool,
    enable_ocr: bool,
    send_log: &dyn Fn(LogMessage),
) -> Result<String, String> {
    let rules = load_rules(rules_path).map_err(|e| format!("加载配置失败: {}", e))?;
    let excel_style = load_excel_style();

    send_log(LogMessage::Info("正在解析 PDF...".to_string()));

    let parser_config = ParserConfig::default();
    let mut data = parse_pdf(pdf_path, enable_ocr, false, &parser_config)
        .map_err(|e| format!("PDF解析失败: {}", e))?;

    send_log(LogMessage::Info(format!(
        "提取 {} 条考核记录",
        data.all_records.len()
    )));

    // 分类
    classify_records(&mut data, &rules);

    // 校验
    let is_valid = validate_amounts(&mut data);
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
        false,
    )
    .map_err(|e| format!("Excel生成失败: {}", e))?;

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

/// 从系统字体目录加载中文字体
fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simfang.ttf",
    ];

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