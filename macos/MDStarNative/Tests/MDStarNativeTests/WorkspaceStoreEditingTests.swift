import Foundation
import XCTest
@testable import MDStarNative

@MainActor
final class WorkspaceStoreEditingTests: XCTestCase {
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
