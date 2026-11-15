// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "TVMaster-Mac",
    // Runs on macOS 15+; on macOS 26 and later the interface uses real Liquid Glass
    // (older systems get the system's translucent materials).
    platforms: [.macOS("15.0")],
    products: [
        .library(name: "TVMasterKit", targets: ["TVMasterKit"]),
        .executable(name: "TVMaster", targets: ["TVMaster"]),
    ],
    targets: [
        .target(name: "TVMasterKit"),
        .executableTarget(name: "TVMaster", dependencies: ["TVMasterKit"]),
        .testTarget(name: "TVMasterKitTests", dependencies: ["TVMasterKit"]),
    ]
)
