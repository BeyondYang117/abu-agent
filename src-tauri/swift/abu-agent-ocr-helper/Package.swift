// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "abu-agent-ocr-helper",
  platforms: [.macOS("14.0")],
  targets: [
    .executableTarget(
      name: "abu-agent-ocr-helper",
      path: "Sources"
    )
  ]
)
