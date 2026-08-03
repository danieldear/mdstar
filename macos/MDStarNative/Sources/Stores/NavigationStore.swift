import Foundation

@MainActor
final class NavigationStore: ObservableObject {
    @Published private(set) var entries: [URL] = []
    @Published private(set) var currentIndex: Int = -1

    var currentURL: URL? {
        guard entries.indices.contains(currentIndex) else { return nil }
        return entries[currentIndex]
    }

    var canGoBack: Bool { currentIndex > 0 }
    var canGoForward: Bool { currentIndex >= 0 && currentIndex < entries.count - 1 }

    func record(_ url: URL) {
        guard currentURL != url else { return }
        if currentIndex + 1 < entries.count {
            entries.removeSubrange((currentIndex + 1)..<entries.count)
        }
        entries.append(url)
        currentIndex = entries.count - 1
    }

    func back() -> URL? {
        guard canGoBack else { return nil }
        currentIndex -= 1
        return currentURL
    }

    func forward() -> URL? {
        guard canGoForward else { return nil }
        currentIndex += 1
        return currentURL
    }
}
