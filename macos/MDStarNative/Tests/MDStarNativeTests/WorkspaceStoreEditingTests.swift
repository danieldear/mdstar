import Foundation
import XCTest
@testable import MDStarNative

@MainActor
final class WorkspaceStoreEditingTests: XCTestCase {
    func testCreateMarkdownFileAddsExtensionOpensTabAndRefreshesWorkspace() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("mdstar-new-file-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }

        let store = WorkspaceStore()
        store.addFolders([folder])
        guard let addedFolder = store.folders.first(where: { $0.url == folder.standardizedFileURL }) else {
            XCTFail("Temporary workspace folder was not added")
            return
        }
        defer { store.removeFolder(addedFolder) }

        let created = try store.createMarkdownFile(
            at: folder.appendingPathComponent("Design Notes")
        )

        XCTAssertEqual(created.lastPathComponent, "Design Notes.md")
        XCTAssertEqual(try String(contentsOf: created, encoding: .utf8), "# Design Notes\n\n")
        XCTAssertEqual(store.viewMode, .split)

        let didOpen = await waitUntil {
            store.selectedURL == created && store.document != nil && !store.isLoading
        }
        XCTAssertTrue(didOpen)
        XCTAssertEqual(store.selectedTabURL, created)
        XCTAssertTrue(store.openDocumentURLs.contains(created))
        XCTAssertFalse(store.isDirty)

        let didRefreshTree = await waitUntil {
            store.folders.first(where: { $0.id == addedFolder.id })?
                .tree?.contains(name: created.lastPathComponent) == true
        }
        XCTAssertTrue(didRefreshTree)
    }

    func testCreateMarkdownFileDoesNotOverwriteExistingDocument() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("mdstar-no-overwrite-\(UUID().uuidString)", isDirectory: true)
        let file = folder.appendingPathComponent("Existing.md")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try "Keep me\n".write(to: file, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: folder) }

        let store = WorkspaceStore()
        XCTAssertThrowsError(try store.createMarkdownFile(at: file))
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), "Keep me\n")
    }

    func testSplitEditsRefreshTheParsedDocumentAndAutosave() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("mdstar-workspace-store-\(UUID().uuidString)", isDirectory: true)
        let file = folder.appendingPathComponent("note.md")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try "# Before\n".write(to: file, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: folder) }

        let store = WorkspaceStore()
        store.openFile(file, recordHistory: false)
        let didLoad = await waitUntil { store.document != nil && !store.isLoading }
        XCTAssertTrue(didLoad)
        let loadedRevision = store.documentRevision

        store.updateSource("# After\n\nSaved from split view.\n")
        XCTAssertTrue(store.isDirty)
        let didRefreshPreview = await waitUntil { store.documentRevision > loadedRevision }
        XCTAssertTrue(didRefreshPreview)
        XCTAssertTrue(store.document?.blocks.first?.searchableText.contains("After") == true)

        let didAutosave = await waitUntil(timeoutNanoseconds: 2_000_000_000) { !store.isDirty }
        XCTAssertTrue(didAutosave)
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), "# After\n\nSaved from split view.\n")
    }

    func testEditorCaretMapsToTheRenderedBlockNearEndOfDocument() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("mdstar-editor-sync-\(UUID().uuidString)", isDirectory: true)
        let file = folder.appendingPathComponent("sync.md")
        let source = "# First\n\nEmoji: 😀\n\n# Last\n\nEdit here.\n"
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try source.write(to: file, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: folder) }

        let store = WorkspaceStore()
        store.openFile(file, recordHistory: false)
        let didLoad = await waitUntil { store.document != nil && !store.isLoading }
        XCTAssertTrue(didLoad)

        store.setEditorCaret(utf16Offset: (source as NSString).length)

        XCTAssertEqual(store.editorFocusedBlockID, store.document?.blocks.last?.id)
        XCTAssertNotEqual(store.editorFocusedBlockID, store.document?.blocks.first?.id)
    }

    private func waitUntil(
        timeoutNanoseconds: UInt64 = 1_000_000_000,
        condition: @escaping @MainActor () -> Bool
    ) async -> Bool {
        let deadline = DispatchTime.now().uptimeNanoseconds + timeoutNanoseconds
        while DispatchTime.now().uptimeNanoseconds < deadline {
            if condition() { return true }
            try? await Task.sleep(nanoseconds: 25_000_000)
        }
        return condition()
    }
}

private extension FileNode {
    func contains(name: String) -> Bool {
        if self.name == name { return true }
        return children?.contains { $0.contains(name: name) } == true
    }
}
