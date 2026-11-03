// swift-tools-version:5.9
// Apple-only runtimes behind C functions, linked into Myna by swift-rs (build.rs).
import PackageDescription

let package = Package(
    name: "MynaSwift",
    platforms: [.macOS("14.2")],
    products: [.library(name: "MynaSwift", type: .static, targets: ["MynaSwift"])],
    targets: [.target(name: "MynaSwift")]
)
