fn main() {
    // ONNX Runtime's Core ML code calls `@available`, whose `__isPlatformVersionAtLeast` lives in clang's
    // runtime; rustc links with -nodefaultlibs, so release (LTO) builds fail without it.
    let out = std::process::Command::new("xcrun").args(["clang", "--print-runtime-dir"]).output().expect("xcrun clang");
    println!("cargo:rustc-link-search=native={}", String::from_utf8(out.stdout).unwrap().trim());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
    tauri_build::build()
}
