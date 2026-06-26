fn main() {
    // 在 OUT_DIR 中编译时嵌入 classify_rules.yaml
    println!("cargo:rerun-if-changed=classify_rules.yaml");
}