import Foundation

struct FileNode: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let path: String
    let kind: String
    let children: [FileNode]?

    var isDirectory: Bool { kind == "directory" }
    var url: URL { URL(fileURLWithPath: path) }
}
