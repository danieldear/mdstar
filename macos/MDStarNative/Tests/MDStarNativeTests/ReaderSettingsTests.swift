import XCTest
@testable import MDStarNative

@MainActor
final class ReaderSettingsTests: XCTestCase {
    private var settings: ReaderSettings!

    override func setUp() async throws {
        // Start from a clean slate so persisted values from a previous run or
        // from the running app cannot influence assertions.
        for key in [
            "mdstar.native.reader.fontFamily",
            "mdstar.native.reader.fontSize",
            "mdstar.native.reader.lineSpacing",
            "mdstar.native.reader.contentWidth",
        ] {
            UserDefaults.standard.removeObject(forKey: key)
        }
        settings = ReaderSettings()
    }

    func testDefaultsAreReadable() {
        XCTAssertEqual(settings.fontSize, ReaderSettings.defaultFontSize)
        XCTAssertEqual(settings.fontFamily, .system)
        XCTAssertGreaterThan(settings.contentWidth, 0)
    }

    func testZoomInAndOutMoveOnePointAtATime() {
        let start = settings.fontSize
        settings.zoomIn()
        XCTAssertEqual(settings.fontSize, start + 1)
        settings.zoomOut()
        XCTAssertEqual(settings.fontSize, start)
    }

    func testZoomClampsToMaximum() {
        for _ in 0..<200 { settings.zoomIn() }
        XCTAssertEqual(settings.fontSize, ReaderSettings.maximumFontSize)
        XCTAssertFalse(settings.canZoomIn)
    }

    func testZoomClampsToMinimum() {
        for _ in 0..<200 { settings.zoomOut() }
        XCTAssertEqual(settings.fontSize, ReaderSettings.minimumFontSize)
        XCTAssertFalse(settings.canZoomOut)
    }

    func testResetZoomReturnsToDefault() {
        settings.zoomIn()
        settings.zoomIn()
        settings.resetZoom()
        XCTAssertEqual(settings.fontSize, ReaderSettings.defaultFontSize)
    }

    /// Direct assignment must be clamped too, not just the zoom helpers.
    func testDirectAssignmentIsClamped() {
        settings.fontSize = 999
        XCTAssertEqual(settings.fontSize, ReaderSettings.maximumFontSize)
        settings.fontSize = -10
        XCTAssertEqual(settings.fontSize, ReaderSettings.minimumFontSize)
    }

    func testSettingsPersistAcrossInstances() {
        settings.fontSize = 19
        settings.fontFamily = .serif

        let reloaded = ReaderSettings()
        XCTAssertEqual(reloaded.fontSize, 19)
        XCTAssertEqual(reloaded.fontFamily, .serif)
    }

    func testHeadingSizesDecreaseWithLevel() {
        // Levels must not collapse to the same size, or the hierarchy vanishes.
        let sizes = (1...6).map { settings.headingPointSize(level: $0) }
        for (smaller, larger) in zip(sizes.dropFirst(), sizes) {
            XCTAssertLessThan(smaller, larger, "heading sizes must strictly decrease")
        }
        XCTAssertGreaterThan(sizes[0], settings.fontSize, "H1 must exceed body size")
    }

    func testHeadingSizesTrackZoom() {
        let before = settings.headingPointSize(level: 1)
        settings.zoomIn()
        XCTAssertGreaterThan(settings.headingPointSize(level: 1), before)
    }

    func testAppearanceColorSchemeMapping() {
        XCTAssertNil(AppearancePreference.system.colorScheme)
        XCTAssertEqual(AppearancePreference.light.colorScheme, .light)
        XCTAssertEqual(AppearancePreference.dark.colorScheme, .dark)
    }
}
