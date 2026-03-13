fn main() {
    let src_dir = std::path::Path::new("src");

    let mut c_config = cc::Build::new();
    c_config.include(&src_dir);
    c_config
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs");
    let parser_path = src_dir.join("parser.c");
    c_config.file(&parser_path);

    // If your language uses an external scanner written in C,
    // then include this block of code:

    /*
    let scanner_path = src_dir.join("scanner.c");
    c_config.file(&scanner_path);
    println!("cargo:rerun-if-changed={}", scanner_path.to_str().unwrap());
    */

    c_config.compile("parser");
    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());

    // If your language uses an external scanner written in C++,
    // then include this block of code:

    let mut cpp_config = cc::Build::new();
    cpp_config.cpp(true);
    cpp_config.include(&src_dir);
    // Only enable the try/catch crash guard on targets that support C++ exceptions.
    // WASI targets compile with -fno-exceptions, so `try` is a compile error there.
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("wasm") {
        cpp_config.define("TREE_SITTER_MARKDOWN_AVOID_CRASH", None);
    }
    cpp_config
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable");
    let scanner_path = src_dir.join("scanner.cc");
    cpp_config.file(&scanner_path);
    cpp_config.compile("scanner");
    println!("cargo:rerun-if-changed={}", scanner_path.to_str().unwrap());

    // When targeting WASI, the linker needs the sysroot lib path for libc++
    if target.contains("wasm") {
        if let Ok(sdk) = std::env::var("WASI_SDK_PATH") {
            println!(
                "cargo:rustc-link-search=native={}/share/wasi-sysroot/lib/{}",
                sdk, target
            );
        }
        println!("cargo:rustc-link-lib=static=c++");
        println!("cargo:rustc-link-lib=static=c++abi");
    }
}
