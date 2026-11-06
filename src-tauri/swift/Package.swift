// swift-tools-version:6.2
// Apple-only runtimes behind C functions, linked into Myna by swift-rs (build.rs).
import PackageDescription

let package = Package(
    name: "MynaSwift",
    platforms: [.macOS("14.2")],
    products: [.library(name: "MynaSwift", type: .static, targets: ["MynaSwift"])],
    dependencies: [.package(url: "https://github.com/Desert-Ant-Labs/desert-ant-core.git", exact: "4.3.0")],
    targets: [.target(name: "MynaSwift", dependencies: [.product(name: "Voz", package: "desert-ant-core")])],
    swiftLanguageModes: [.v5]
)
