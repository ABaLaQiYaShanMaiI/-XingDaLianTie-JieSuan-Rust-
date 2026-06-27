//! 构建脚本
//! ========
//! 监听 classify_rules.yaml 变化，供 rerun-if-changed 使用。
//! 规则文件的嵌入通过 src/config.rs 中的 include_str!() 实现，
//! 在编译时将 classify_rules.yaml 的内容直接嵌入二进制，
//! 确保单个可执行文件在无外部配置时也能自包含运行。
//!
//! Windows 7 兼容性配置见 .cargo/config.toml
//! (rust-lld + /alternatename 将 IAT 中的
//!  GetSystemTimePreciseAsFileTime 重定向为 GetSystemTimeAsFileTime)

fn main() {
    println!("cargo:rerun-if-changed=classify_rules.yaml");
}