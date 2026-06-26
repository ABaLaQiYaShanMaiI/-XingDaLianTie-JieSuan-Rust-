//! GUI 模块
//! =======
//! 提供图形化用户界面 (egui/eframe)。
//! 支持: 文件选择、PDF 处理、实时日志显示。

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
        }
    }
}

impl eframe::App for XingDaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 轮询后台处理结果
        self.poll_results();

        // ---- 顶部面板 ----
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.heading(format!("兴达炼铁保产事业部 · 结算单明细工具 v{}", VERSION));
            ui.label("PDF 结算单 → 自动提取考核明细 → 生成 Excel");
            if !self.processing {
                ui.colored_label(egui::Color32::from_rgb(0, 120, 212), "📂 拖拽 PDF 文件到此处");
            }
        });

        // ---- 中央面板 ----
        egui::CentralPanel::default().show(ctx, |ui| {
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
                ui.checkbox(&mut self.validate_only, "仅校验不生成 Excel（--validate-only）");
                ui.checkbox(&mut self.dump_text, "同时导出原始文本（--dump-text）");
                ui.checkbox(&mut self.no_summary, "不生成汇总信息区域（--no-summary）");
                ui.checkbox(&mut self.enable_ocr, "启用 OCR（PDF 无文本层时使用 Tesseract+Ghostscript）");
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
            if let Some(ref output) = self.last_output {
                ui.colored_label(
                    egui::Color32::from_rgb(106, 153, 85),
                    format!("✅ 输出: {}", output),
                );
            } else {
                ui.colored_label(egui::Color32::GRAY, "就绪");
            }
        });
    }
}

impl XingDaApp {
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

            // 处理
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
                        send_log(LogMessage::Warning("⚠ 处理完成但未生成文件".to_string()));
                        let _ = result_tx.send(Err("无文件生成".to_string()));
                    } else {
                        send_log(LogMessage::Success(format!("✅ 完成！输出文件: {}", output)));
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
        // 接收日志
        if let Some(ref rx) = self.log_receiver {
            while let Ok(msg) = rx.try_recv() {
                self.log_messages.push(msg);
            }
        }

        // 接收结果
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

    // 确保目录存在
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    generate_excel(
        &data,
        output_path_str,
        &rules.area_order,
        &excel_style,
        include_summary,
        false, // GUI mode always includes detail sheets
    )
    .map_err(|e| format!("Excel生成失败: {}", e))?;

    Ok(output_path_str.to_string())
}

/// 启动 GUI
pub fn launch_gui() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("兴达炼铁保产事业部 结算单明细工具")
            .with_inner_size([680.0, 620.0])
            .with_min_inner_size([600.0, 500.0]),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "XingDa JieSuan",
        options,
        Box::new(|cc| {
            // 加载中文字体以解决 GUI 乱码
            setup_chinese_fonts(&cc.egui_ctx);
            Ok(Box::new(XingDaApp::default()))
        }),
    ) {
        eprintln!("GUI 启动失败: {}", e);
    }
}

/// 从系统字体目录加载中文字体
///
/// 自动尝试以下字体（优先级递减）：
/// 1. 微软雅黑（推荐）
/// 2. 宋体
/// 3. 黑体
/// 4. 仿宋
///
/// 如果所有字体加载失败，使用 egui 默认字体（不阻止启动）
///
/// # 兼容性说明
///
/// Windows 7/8+ 均包含微软雅黑，无需担心字体缺失。
/// 但建议在 README 中说明字体依赖。
fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Windows 7/8+ 均包含微软雅黑，无需担心字体缺失
    // 但建议在 README 中说明字体依赖
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",   // 微软雅黑
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",  // 宋体
        "C:\\Windows\\Fonts\\simhei.ttf",  // 黑体
        "C:\\Windows\\Fonts\\simfang.ttf", // 仿宋
    ];

    let font_name = "chinese_font";
    for path in &font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            info!("已加载中文字体: {}", path);

            fonts.font_data.insert(
                font_name.to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes.to_vec())),
            );

            // 将中文字体插入到 Proportional 和 Monospace 字体族的最前面
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
