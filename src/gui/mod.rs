//! 图形化界面 (egui/eframe)：文件选择、PDF 处理、实时日志、拖拽、环境检测。

mod app;
mod components;
mod fonts;

pub use app::XingDaApp;
pub use fonts::setup_chinese_fonts;

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