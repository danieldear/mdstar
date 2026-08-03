// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MDStarNative",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "MDStarNative", targets: ["MDStarNative"])],
    targets: [
        .executableTarget(
            name: "MDStarNative",
            path: "Sources",
            linkerSettings: [
                // `target/ffi` is searched first: release packaging stages a
                // universal libmdstar_ffi there so multi-arch builds link. Plain
                // `swift build` falls back to the cargo debug dylib.
                .unsafeFlags([
                    "-L../../target/ffi",
                    "-L../../target/debug",
                    "-lmdstar_ffi",
                    "-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"
                ]),
                .linkedFramework("AppKit")
            ]
        ),
        .testTarget(name: "MDStarNativeTests", dependencies: ["MDStarNative"], path: "Tests/MDStarNativeTests")
    ]
)
