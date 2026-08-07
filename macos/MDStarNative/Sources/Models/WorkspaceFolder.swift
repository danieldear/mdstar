import Foundation

/// A root folder in the workspace.
///
/// The workspace is a set of these rather than a single directory, so unrelated
/// project folders can be open side by side and added or removed independently.
struct WorkspaceFolder: Identifiable, Hashable, Sendable {
    let url: URL
    var tree: FileNode?
    /// Set when a rescan fails, e.g. the folder was moved, renamed or unmounted.
    var isUnavailable = false

    var id: String { url.standardizedFileURL.path }
    var name: String { url.lastPathComponent }

    init(url: URL, tree: FileNode? = nil, isUnavailable: Bool = false) {
        self.url = url.standardizedFileURL
        self.tree = tree
        self.isUnavailable = isUnavailable
    }
}
