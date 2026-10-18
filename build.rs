fn main() {
    // "qt" stili sistemdeki Qt/Breeze görünümünü kullanır. Qt yoksa: UDF_STYLE=fluent cargo run
    let style = std::env::var("UDF_STYLE").unwrap_or_else(|_| "qt".into());
    println!("cargo:rerun-if-env-changed=UDF_STYLE");
    let config = slint_build::CompilerConfiguration::new().with_style(style);
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
}
