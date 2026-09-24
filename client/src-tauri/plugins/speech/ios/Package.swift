// swift-tools-version:5.9

import PackageDescription

let package = Package(
    name: "tauri-plugin-hippocampus-speech",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        .library(
            name: "tauri-plugin-hippocampus-speech",
            type: .static,
            targets: ["tauri-plugin-hippocampus-speech"])
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api"),
        // Parakeet v3 on the Neural Engine. Pinned exactly: its API moved
        // twice in the releases before this one.
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.1"),
    ],
    targets: [
        .target(
            name: "tauri-plugin-hippocampus-speech",
            dependencies: [
                .byName(name: "Tauri"),
                .product(name: "FluidAudio", package: "FluidAudio"),
            ],
            path: "Sources")
    ]
)
