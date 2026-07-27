import AppKit
import Foundation

private struct BackgroundFailure: Error, Sendable {
    let message: String
}

private struct LoadedDocument: Sendable {
    let ir: DocumentIR
    let rawText: String
}

@MainActor
final class WorkspaceStore: ObservableObject {
    @Published private(set) var workspaceURL: URL?
    @Published private(set) var fileTree: FileNode?
    @Published private(set) var document: DocumentIR?
    @Published private(set) var selectedURL: URL?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false
    @Published private(set) var expandedFileIDs: Set<String> = []
    @Published private(set) var focusedBlockID: String?
    @Published private(set) var activeHeadingID: String?
    @Published private(set) var rawText: String = ""
    @Published var viewMode: ReaderViewMode = .view

    // In-document find
    @Published var isFindPresented = false
    @Published var findQuery = ""
    @Published private(set) var findMatches: [String] = []
    @Published private(set) var findIndex = 0

    // Section bookmarks (per document)
    @Published private(set) var bookmarks: [Bookmark] = []

    private let service = RustDocumentService()
    private let bookmarkStore = SecurityScopedBookmarkStore()
    private let fileWatcher = FileWatcher()
    private let workspaceKey = "mdstar.native.workspace.path"
    private let selectedFileKey = "mdstar.native.workspace.selectedFile"
    private var scopedWorkspaceURL: URL?
    private var workspaceOperation = 0
    private var documentOperation = 0

    init() {
        let defaults = UserDefaults.standard
        let path = defaults.string(forKey: workspaceKey) ?? ""
        if !path.isEmpty { workspaceURL = URL(fileURLWithPath: path) }
    }

    func restoreWorkspaceIfNeeded() {
        guard let workspaceURL else { return }
        activateWorkspaceAccess(workspaceURL)
        openWorkspace(workspaceURL, restoreSelection: true)
    }

    func chooseWorkspace() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Open Workspace"
        if panel.runModal() == .OK, let url = panel.url {
            openWorkspace(url)
        }
    }

    func chooseFile() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Open File"
        if panel.runModal() == .OK, let url = panel.url {
            openFile(url, recordHistory: true)
        }
    }

    func openWorkspace(_ url: URL, restoreSelection: Bool = false) {
        workspaceURL = url.standardizedFileURL
        activateWorkspaceAccess(workspaceURL!)
        UserDefaults.standard.set(workspaceURL?.path, forKey: workspaceKey)
        bookmarkStore.save(workspaceURL!)
        expandedFileIDs = loadExpandedIDs(for: workspaceURL!)
        isLoading = true
        errorMessage = nil
        workspaceOperation += 1
        let operation = workspaceOperation
        Task { [weak self, service] in
            let result: Result<FileNode, BackgroundFailure> = await Task.detached {
                do { return .success(try service.workspaceTree(at: url)) }
                catch { return .failure(BackgroundFailure(message: error.localizedDescription)) }
            }.value
            guard let self, self.workspaceOperation == operation else { return }
            self.isLoading = false
            switch result {
            case .success(let tree):
                self.fileTree = tree
                if restoreSelection,
                   let saved = UserDefaults.standard.string(forKey: self.selectedFileKey),
                   FileManager.default.fileExists(atPath: saved) {
                    self.openFile(URL(fileURLWithPath: saved), recordHistory: true)
                }
            case .failure(let error):
                self.errorMessage = error.message
            }
        }
    }

    func closeWorkspace() {
        fileWatcher.stop()
        scopedWorkspaceURL?.stopAccessingSecurityScopedResource()
        scopedWorkspaceURL = nil
        workspaceURL = nil
        fileTree = nil
        selectedURL = nil
        document = nil
        UserDefaults.standard.removeObject(forKey: workspaceKey)
        UserDefaults.standard.removeObject(forKey: selectedFileKey)
        bookmarkStore.remove()
        expandedFileIDs.removeAll()
    }

    func openFile(_ url: URL, recordHistory: Bool) {
        let normalized = url.standardizedFileURL
        isLoading = true
        errorMessage = nil
        documentOperation += 1
        let operation = documentOperation
        Task { [weak self, service] in
            let result: Result<LoadedDocument, BackgroundFailure> = await Task.detached {
                do {
                    let ir = try service.loadDocument(at: normalized)
                    let raw = (try? String(contentsOf: normalized, encoding: .utf8)) ?? ""
                    return .success(LoadedDocument(ir: ir, rawText: raw))
                } catch {
                    return .failure(BackgroundFailure(message: error.localizedDescription))
                }
            }.value
            guard let self, self.documentOperation == operation else { return }
            self.isLoading = false
            switch result {
            case .success(let loaded):
                self.document = loaded.ir
                self.rawText = loaded.rawText
                self.activeHeadingID = loaded.ir.outline.first?.id
                self.selectedURL = normalized
                self.loadBookmarks(for: normalized)
                self.resetFind()
                // Auto-reload when the open file changes on disk. Starting a
                // new watch stops any previous one, so switching documents
                // rebinds cleanly.
                self.fileWatcher.start(url: normalized) { [weak self] in
                    guard let self, self.selectedURL == normalized else { return }
                    self.reload()
                }
                UserDefaults.standard.set(normalized.path, forKey: self.selectedFileKey)
                if recordHistory { NotificationCenter.default.post(name: .mdstarOpenedDocument, object: normalized) }
            case .failure(let error):
                self.errorMessage = error.message
            }
        }
    }

    func setActiveHeading(_ id: String?) {
        guard activeHeadingID != id else { return }
        activeHeadingID = id
    }

    /// Spacebar behavior inspired by Mud: flip between reading and split (source)
    /// views.
    func toggleSourcePreview() {
        viewMode = viewMode == .view ? .split : .view
    }

    // MARK: - Find

    func presentFind() {
        isFindPresented = true
        recomputeFind()
    }

    func dismissFind() {
        isFindPresented = false
    }

    private func resetFind() {
        findQuery = ""
        findMatches = []
        findIndex = 0
    }

    func recomputeFind() {
        let needle = findQuery.trimmingCharacters(in: .whitespaces).lowercased()
        guard needle.count >= 1, let document else {
            findMatches = []
            findIndex = 0
            return
        }
        var ids: [String] = []
        func walk(_ blocks: [BlockIR]) {
            for block in blocks {
                if block.searchableText.lowercased().contains(needle) {
                    ids.append(block.id)
                }
                walk(block.children)
                walk(block.items.flatMap(\.children))
            }
        }
        walk(document.blocks)
        findMatches = ids
        findIndex = 0
        focusCurrentMatch()
    }

    func findNext() {
        guard !findMatches.isEmpty else { return }
        findIndex = (findIndex + 1) % findMatches.count
        focusCurrentMatch()
    }

    func findPrevious() {
        guard !findMatches.isEmpty else { return }
        findIndex = (findIndex - 1 + findMatches.count) % findMatches.count
        focusCurrentMatch()
    }

    private func focusCurrentMatch() {
        guard findMatches.indices.contains(findIndex) else { return }
        focus(blockID: findMatches[findIndex])
    }

    var currentMatchID: String? {
        findMatches.indices.contains(findIndex) ? findMatches[findIndex] : nil
    }

    // MARK: - Bookmarks

    private func bookmarksKey(for url: URL) -> String {
        "mdstar.native.bookmarks.\(url.standardizedFileURL.path)"
    }

    private func loadBookmarks(for url: URL) {
        guard let data = UserDefaults.standard.data(forKey: bookmarksKey(for: url)),
              let decoded = try? JSONDecoder().decode([Bookmark].self, from: data) else {
            bookmarks = []
            return
        }
        bookmarks = decoded
    }

    private func persistBookmarks() {
        guard let url = selectedURL else { return }
        let data = try? JSONEncoder().encode(bookmarks)
        UserDefaults.standard.set(data, forKey: bookmarksKey(for: url))
    }

    func isBookmarked(_ id: String) -> Bool {
        bookmarks.contains { $0.id == id }
    }

    func toggleBookmark(id: String, title: String, level: Int) {
        if let index = bookmarks.firstIndex(where: { $0.id == id }) {
            bookmarks.remove(at: index)
        } else {
            bookmarks.append(Bookmark(id: id, title: title, level: level))
            // Keep bookmarks in document order.
            if let outline = document?.outline {
                let order = Dictionary(uniqueKeysWithValues: outline.enumerated().map { ($0.element.id, $0.offset) })
                bookmarks.sort { (order[$0.id] ?? 0) < (order[$1.id] ?? 0) }
            }
        }
        persistBookmarks()
    }

    func removeBookmark(_ id: String) {
        bookmarks.removeAll { $0.id == id }
        persistBookmarks()
    }

    private func activateWorkspaceAccess(_ url: URL) {
        if scopedWorkspaceURL != url {
            scopedWorkspaceURL?.stopAccessingSecurityScopedResource()
            _ = url.startAccessingSecurityScopedResource()
            scopedWorkspaceURL = url
        }
    }

    func isExpanded(_ id: String) -> Bool {
        expandedFileIDs.contains(id)
    }

    func setExpanded(_ id: String, expanded: Bool) {
        if expanded { expandedFileIDs.insert(id) } else { expandedFileIDs.remove(id) }
        guard let workspaceURL else { return }
        UserDefaults.standard.set(Array(expandedFileIDs), forKey: expandedKey(for: workspaceURL))
    }

    func focus(blockID: String) {
        focusedBlockID = blockID
    }

    private func loadExpandedIDs(for url: URL) -> Set<String> {
        Set(UserDefaults.standard.stringArray(forKey: expandedKey(for: url)) ?? [])
    }

    private func expandedKey(for url: URL) -> String {
        "mdstar.native.workspace.expanded.\(url.path)"
    }

    func reload() {
        guard let selectedURL else { return }
        openFile(selectedURL, recordHistory: false)
    }
}

extension Notification.Name {
    static let mdstarOpenedDocument = Notification.Name("mdstarOpenedDocument")
}
