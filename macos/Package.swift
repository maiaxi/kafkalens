// swift-tools-version: 6.0
// Run `scripts/build-core.sh` first: it produces RustLib/ (the Rust core and
// its generated Swift bindings), which this package links against.
import PackageDescription

let package = Package(
    name: "Kafkalens",
    platforms: [.macOS(.v14)],
    targets: [
        .binaryTarget(
            name: "KafkalensCoreFFI",
            path: "RustLib/KafkalensCoreFFI.xcframework"
        ),
        // UniFFI-generated bindings; Swift 5 mode because the generated code
        // is not written for Swift 6 strict concurrency.
        .target(
            name: "KafkalensCore",
            dependencies: ["KafkalensCoreFFI"],
            path: "RustLib/KafkalensCore",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "Kafkalens",
            dependencies: ["KafkalensCore"],
            path: "Sources/Kafkalens"
        ),
    ]
)
