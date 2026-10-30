// swift-tools-version: 5.9
import PackageDescription

// Desert Ant's package needs Swift 6.2. An older toolchain (CI's macos-14 runner) still builds
// this bridge, without the SDK, and Voz reports itself unavailable.
#if compiler(>=6.2)
let sdk: [Package.Dependency] = [
    .package(url: "https://github.com/Desert-Ant-Labs/desert-ant-core.git", exact: "4.2.1"),
]
let voz: [Target.Dependency] = [
    .product(name: "Voz", package: "desert-ant-core"),
    .product(name: "Uhm", package: "desert-ant-core"),
    .product(name: "Who", package: "desert-ant-core"),
]
#else
let sdk: [Package.Dependency] = []
let voz: [Target.Dependency] = []
#endif

let package = Package(
    name: "MynaBridge",
    platforms: [.macOS("14.2")],
    products: [.library(name: "MynaBridge", type: .static, targets: ["MynaBridge"])],
    dependencies: sdk,
    targets: [.target(name: "MynaBridge", dependencies: voz)],
    swiftLanguageVersions: [.v5]
)
