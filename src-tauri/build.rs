fn main() {
    // Builds swift/ (Apple Speech) and links the Swift runtime. It also links clang's runtime, which
    // ONNX Runtime's Core ML code needs for `@available` (rustc links with -nodefaultlibs).
    swift_rs::SwiftLinker::new("14.2").with_package("MynaSwift", "swift").link();
    // SwiftPM 27 writes Products/<Config>/; swift-rs only looks there when `xcodebuild` exists,
    // which the Command Line Tools lack.
    let config = if std::env::var("DEBUG").as_deref() == Ok("true") { "Debug" } else { "Release" };
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={out}/swift-rs/MynaSwift/out/Products/{config}");
    println!("cargo:rerun-if-changed=swift/Sources");
    println!("cargo:rerun-if-changed=swift/Package.swift");
    tauri_build::build()
}
