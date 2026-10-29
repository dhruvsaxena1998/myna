fn main() {
    // Builds the Swift package (Desert Ant SDK bridge) and links it into the app.
    swift_rs::SwiftLinker::new("14.2").with_package("MynaBridge", "swift").link();
    // swift-rs looks for the archive in SwiftPM's classic layout only; the `release`/`debug`
    // symlink exists in every layout.
    let config = if std::env::var("DEBUG").is_ok_and(|d| d == "true") { "debug" } else { "release" };
    println!("cargo:rustc-link-search=native={}/swift-rs/MynaBridge/{config}", std::env::var("OUT_DIR").unwrap());
    // The Swift runtime ships with macOS; swift-rs doesn't add it to the rpath.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    tauri_build::build()
}
