//! 跨平台中文字体加载

use log::{info, warn};

/// 从系统字体目录加载中文字体（跨平台）
///
/// 按优先级依次尝试系统常见中文字体路径，首个存在的字体即被加载并设置为默认。
pub fn setup_chinese_fonts(ctx: &egui::Context) {
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