//! 构建脚本
//! ========
//! 监听 classify_rules.yaml 变化，供 Rerun-if-changed 使用。
//! 规则文件的嵌入通过 src/config.rs 中的 include_str!() 实现，
//! 在编译时将 classify_rules.yaml 的内容直接嵌入二进制，
//! 确保单个可执行文件在无外部配置时也能自包含运行。

fn main() {
    // 当 classify_rules.yaml 变化时重新编译
    println!("cargo:rerun-if-changed=classify_rules.yaml");

    // Windows 7 兼容性配置见 .cargo/config.toml
    // (rustflags 方式不会像 cargo:rustc-link-arg 那样泄露到
    //  依赖 crate 的 build scripts，避免 WinMain 链接错误)
}
