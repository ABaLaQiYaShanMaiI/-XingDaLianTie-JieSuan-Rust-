//! PDF结算单转Excel明细工具。无参数启动 GUI，有参数启动 CLI。支持跨平台。

use std::env;

fn main() {
    // 检测命令行参数
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        // 有参数 → CLI 模式
        if let Err(e) = xingda_jiesuan::cli::run_cli() {
            eprintln!("错误: {}", e);
            std::process::exit(1);
        }
    } else {
        // 无参数 → GUI 模式
        xingda_jiesuan::gui::launch_gui();
    }
}