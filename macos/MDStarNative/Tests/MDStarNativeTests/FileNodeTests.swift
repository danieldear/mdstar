import XCTest
@testable import MDStarNative

final class FileNodeTests: XCTestCase {
    func testDirectoryAndFileIdentityAreStable() {
        let file = FileNode(id: "/workspace/readme.md", name: "readme.md", path: "/workspace/readme.md", kind: "file", children: nil)
        let directory = FileNode(id: "/workspace/docs", name: "docs", path: "/workspace/docs", kind: "directory", children: [file])
        XCTAssertTrue(directory.isDirectory)
        XCTAssertFalse(file.isDirectory)
        XCTAssertEqual(directory.children?.first?.id, file.id)
    }
}
