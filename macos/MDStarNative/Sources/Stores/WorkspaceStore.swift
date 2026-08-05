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
    /// Root folders in the workspace, each rendered as its own tree. Multiple
    /// roots let unrelated project directories sit side by side.
    @Published private(set) var folders: [WorkspaceFolder] = []
    @Published private(set) var document: DocumentIR?
    @Published private(set) var selectedURL: URL?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false
    @Published private(set) var expandedFileIDs: Set<String> = []
    @Published private(set) var focusedBlockID: String?
    @Published private(set) var activeHeadingID: String?
    @Published private(set) var rawText: String = ""
    @Published private(set) var isDirty = false
    /// URLs currently open in this window. Documents load on selection so the
    /// workspace remains responsive even with many large files open.
    @Published private(set) var openDocumentURLs: [URL] = []
    @Published private(set) var selectedTabURL: URL?
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
    private let directoryWatcher = DirectoryWatcher()
    private let foldersKey = "mdstar.native.workspace.folders"
    private let selectedFileKey = "mdstar.native.workspace.selectedFile"
    private var scopedWorkspaceURL: URL?
    private var workspaceOperation = 0
    private var documentOperation = 0
    private var sourceRevision = 0
    private var sourceParseTask: Task<Void, Never>?
    private var autoSaveTask: Task<Void, Never>?

    init() {
        let paths = UserDefaults.standard.stringArray(forKey: foldersKey) ?? []
        folders = paths.map { WorkspaceFolder(url: URL(fileURLWithPath: $0)) }
    }

    /// Folder that contains the open document, used for breadcrumbs.
    var workspaceURL: URL? {
        guard let selectedURL else { return folders.first?.url }
        let path = selectedURL.standardizedFileURL.path
        return folders.first { path.hasPrefix($0.url.standardizedFileURL.path) }?.url
            ?? folders.first?.url
    }

    func restoreWorkspaceIfNeeded() {
        guard !folders.isEmpty else { return }
        for folder in folders { activateWorkspaceAccess(folder.url) }
        refreshFolders(restoreSelection: true)
        startWatchingFolders()
    }

    /// Prompts for one or more folders to add as workspace roots.
    func chooseWorkspace() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        panel.prompt = "Add Folder"
        panel.message = "Choose folders to add to the workspace"
        if panel.runModal() == .OK {
            addFolders(panel.urls)
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

    // MARK: - Workspace folders

    func addFolders(_ urls: [URL]) {
        var added = false
        for url in urls {
            let normalized = url.standardizedFileURL
            guard !folders.contains(where: { $0.url == normalized }) else { continue }
            activateWorkspaceAccess(normalized)
            bookmarkStore.save(normalized)
            folders.append(WorkspaceFolder(url: normalized))
            expandedFileIDs.formUnion(loadExpandedIDs(for: normalized))
            added = true
        }
        guard added else { return }
        persistFolders()
        refreshFolders()
        startWatchingFolders()
    }

    func removeFolder(_ folder: WorkspaceFolder) {
        folders.removeAll { $0.id == folder.id }
        persistFolders()
        startWatchingFolders()
        if folders.isEmpty { directoryWatcher.stop() }
    }

    /// Legacy single-folder entry point, kept for `onOpenURL` and drops.
    func openWorkspace(_ url: URL, restoreSelection: Bool = false) {
        addFolders([url])
        if restoreSelection { restoreSelectionIfPossible() }
    }

    /// Rescans every root. Called on demand and whenever the watcher fires, so
    /// files created outside the app appear without reopening the folder.
    func refreshFolders(restoreSelection: Bool = false) {
        guard !folders.isEmpty else { return }
        workspaceOperation += 1
        let operation = workspaceOperation
        let roots = folders.map(\.url)

        Task { [weak self, service] in
            let scanned: [(URL, FileNode?)] = await Task.detached {
                roots.map { url in (url, try? service.workspaceTree(at: url)) }
            }.value
            guard let self, self.workspaceOperation == operation else { return }
            for (url, tree) in scanned {
                guard let index = self.folders.firstIndex(where: { $0.url == url }) else { continue }
                self.folders[index].tree = tree
                self.folders[index].isUnavailable = tree == nil
            }
            if restoreSelection { self.restoreSelectionIfPossible() }
        }
    }

    private func restoreSelectionIfPossible() {
        guard selectedURL == nil,
              let saved = UserDefaults.standard.string(forKey: selectedFileKey),
              FileManager.default.fileExists(atPath: saved) else { return }
        openFile(URL(fileURLWithPath: saved), recordHistory: true)
    }

    private func persistFolders() {
        UserDefaults.standard.set(folders.map(\.url.path), forKey: foldersKey)
    }

    private func startWatchingFolders() {
        let roots = folders.map(\.url)
        guard !roots.isEmpty else {
            directoryWatcher.stop()
            return
        }
        directoryWatcher.watch(roots) { [weak self] in
            self?.refreshFolders()
        }
    }

    func closeWorkspace() {
        guard save() else { return }
        fileWatcher.stop()
        directoryWatcher.stop()
        scopedWorkspaceURL?.stopAccessingSecurityScopedResource()
        scopedWorkspaceURL = nil
        folders = []
        selectedURL = nil
        selectedTabURL = nil
        openDocumentURLs = []
        document = nil
        rawText = ""
        bookmarks = []
        UserDefaults.standard.removeObject(forKey: foldersKey)
        UserDefaults.standard.removeObject(forKey: selectedFileKey)
        bookmarkStore.remove()
        expandedFileIDs.removeAll()
    }

    func openFile(_ url: URL, recordHistory: Bool) {
        let normalized = url.standardizedFileURL
        if selectedURL != normalized, isDirty, !save() { return }
        cancelEditingTasks()
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
                self.isDirty = false
                self.activeHeadingID = loaded.ir.outline.first?.id
                self.selectedURL = normalized
                self.selectedTabURL = normalized
                if !self.openDocumentURLs.contains(normalized) {
                    self.openDocumentURLs.append(normalized)
                }
                self.loadBookmarks(for: normalized)
                self.resetFind()
                // Auto-reload when the open file changes on disk. Starting a
                // new watch stops any previous one, so switching documents
                // rebinds cleanly.
                self.fileWatcher.start(url: normalized) { [weak self] in
                    guard let self, !self.isDirty, self.selectedURL == normalized else { return }
                    self.reload()
                }
                UserDefaults.standard.set(normalized.path, forKey: self.selectedFileKey)
                if recordHistory { NotificationCenter.default.post(name: .mdstarOpenedDocument, object: normalized) }
            case .failure(let error):
                self.errorMessage = error.message
            }
        }
    }

    func selectTab(_ url: URL) {
        let normalized = url.standardizedFileURL
        guard openDocumentURLs.contains(normalized), selectedURL != normalized else { return }
        openFile(normalized, recordHistory: true)
    }

    func closeTab(_ url: URL) {
        let normalized = url.standardizedFileURL
        guard let closingIndex = openDocumentURLs.firstIndex(of: normalized) else { return }
        let wasSelected = selectedURL == normalized
        if wasSelected, isDirty, !save() { return }
        openDocumentURLs.remove(at: closingIndex)

        guard wasSelected else { return }
        guard !openDocumentURLs.isEmpty else {
            clearOpenDocument()
            return
        }

        let nextIndex = min(closingIndex, openDocumentURLs.count - 1)
        openFile(openDocumentURLs[nextIndex], recordHistory: false)
    }

    private func clearOpenDocument() {
        fileWatcher.stop()
        selectedURL = nil
        selectedTabURL = nil
        document = nil
        rawText = ""
        isDirty = false
        errorMessage = nil
        activeHeadingID = nil
        focusedBlockID = nil
        bookmarks = []
        resetFind()
        UserDefaults.standard.removeObject(forKey: selectedFileKey)
    }

    func setActiveHeading(_ id: String?) {
        guard activeHeadingID != id else { return }
        activeHeadingID = id
    }

    /// Switch between rendered reading and the editable source split view.
    func toggleSourcePreview() {
        viewMode = viewMode == .view ? .split : .view
    }

    // MARK: - Editing and saving

    /// Updates the in-memory source immediately, then debounces parsing and
    /// saving so split preview remains responsive while typing.
    func updateSource(_ source: String) {
        guard rawText != source else { return }
        rawText = source
        isDirty = true
        sourceRevision += 1

        scheduleLivePreview(source, revision: sourceRevision)
        scheduleAutosave(revision: sourceRevision)
    }

    /// Saves the current source atomically. Returns false when disk write
    /// fails, allowing tab/workspace changes to keep the editor open instead
    /// of silently dropping unsaved work.
    @discardableResult
    func save() -> Bool {
        guard isDirty else { return true }
        guard let selectedURL else { return false }

        do {
            try rawText.write(to: selectedURL, atomically: true, encoding: .utf8)
            isDirty = false
            errorMessage = nil
            return true
        } catch {
            errorMessage = "Could not save \(selectedURL.lastPathComponent): \(error.localizedDescription)"
            return false
        }
    }

    private func scheduleLivePreview(_ source: String, revision: Int) {
        sourceParseTask?.cancel()
        let origin = selectedURL?.path ?? "untitled.md"
        let service = service

        sourceParseTask = Task { [weak self] in
            do {
                try await Task.sleep(nanoseconds: 140_000_000)
            } catch {
                return
            }
            guard !Task.isCancelled else { return }

            let result: Result<DocumentIR, BackgroundFailure> = await Task.detached(priority: .userInitiated) {
                do {
                    return .success(try service.parseDocument(source, origin: origin))
                } catch {
                    return .failure(BackgroundFailure(message: error.localizedDescription))
                }
            }.value

            guard let self, !Task.isCancelled, self.sourceRevision == revision else { return }
            switch result {
            case .success(let document):
                self.document = document
                self.activeHeadingID = document.outline.first?.id
                if self.isFindPresented { self.recomputeFind() }
            case .failure(let error):
                // Keep the last valid preview visible while the user is in the
                // middle of temporarily incomplete Markdown input.
                self.errorMessage = error.message
            }
        }
    }

    private func scheduleAutosave(revision: Int) {
        autoSaveTask?.cancel()
        autoSaveTask = Task { [weak self] in
            do {
                try await Task.sleep(nanoseconds: 750_000_000)
            } catch {
                return
            }
            guard let self, !Task.isCancelled, self.sourceRevision == revision else { return }
            _ = self.save()
        }
    }

    private func cancelEditingTasks() {
        sourceRevision += 1
        sourceParseTask?.cancel()
        autoSaveTask?.cancel()
        sourceParseTask = nil
        autoSaveTask = nil
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

    /// Recent search terms, newest first. Mirrors the NSSearchField recents
    /// menu so history survives relaunches even if the field is rebuilt.
    private let recentSearchesKey = "mdstar.native.find.recents"

    var hasActiveSearch: Bool {
        !findQuery.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var matchSummary: String {
        guard hasActiveSearch else { return "" }
        guard !findMatches.isEmpty else { return "No results" }
        return "\(findIndex + 1) of \(findMatches.count)"
    }

    /// The web engine finds and counts matches itself, so it reports results
    /// back rather than the store recomputing them from the IR.
    func reportFindResults(count: Int, index: Int) {
        findMatches = count > 0 ? Array(repeating: "", count: count) : []
        findIndex = max(0, index - 1)
    }

    func rememberSearch(_ term: String) {
        let trimmed = term.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        var recents = UserDefaults.standard.stringArray(forKey: recentSearchesKey) ?? []
        recents.removeAll { $0.caseInsensitiveCompare(trimmed) == .orderedSame }
        recents.insert(trimmed, at: 0)
        UserDefaults.standard.set(Array(recents.prefix(6)), forKey: recentSearchesKey)
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
        for folder in folders {
            UserDefaults.standard.set(Array(expandedFileIDs), forKey: expandedKey(for: folder.url))
        }
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
        guard !isDirty else { return }
        if let diskText = try? String(contentsOf: selectedURL, encoding: .utf8), diskText == rawText {
            return
        }
        openFile(selectedURL, recordHistory: false)
    }
}

extension Notification.Name {
    static let mdstarOpenedDocument = Notification.Name("mdstarOpenedDocument")
}
