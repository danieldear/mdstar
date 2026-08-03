import Foundation

/// Keeps workspace authorization ready for sandboxed distribution while staying
/// harmless in local unsigned development builds.
final class SecurityScopedBookmarkStore {
    private let bookmarkKey = "mdstar.native.workspace.bookmark"

    func save(_ url: URL) {
        guard let data = try? url.bookmarkData(
            options: [.withSecurityScope],
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        ) else { return }
        UserDefaults.standard.set(data, forKey: bookmarkKey)
    }

    func restore() -> URL? {
        guard let data = UserDefaults.standard.data(forKey: bookmarkKey) else { return nil }
        var stale = false
        guard let url = try? URL(
            resolvingBookmarkData: data,
            options: [.withSecurityScope],
            relativeTo: nil,
            bookmarkDataIsStale: &stale
        ) else { return nil }
        if stale { save(url) }
        return url
    }

    func remove() {
        UserDefaults.standard.removeObject(forKey: bookmarkKey)
    }
}
