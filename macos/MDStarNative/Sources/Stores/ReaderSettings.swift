import AppKit
import SwiftUI

/// Reading typeface. Values are stored as raw strings so the choice survives in
/// `UserDefaults` even if the list changes.
enum ReaderFontFamily: String, CaseIterable, Identifiable {
    case system
    case serif
    case rounded
    case monospaced

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: "System"
        case .serif: "Serif"
        case .rounded: "Rounded"
        case .monospaced: "Monospaced"
        }
    }

    var design: Font.Design {
        switch self {
        case .system: .default
        case .serif: .serif
        case .rounded: .rounded
        case .monospaced: .monospaced
        }
    }

    /// Sample shown in the settings picker so the choice is visible before use.
    var sample: String { "The quick brown fox" }
}

/// Reader chrome theme. `system` follows the macOS appearance; the others pin it.
enum AppearancePreference: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: Self { self }

    var title: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

/// Typography and appearance preferences for the reader.
///
/// Backed by `UserDefaults` through `@AppStorage`-equivalent manual storage so
/// the values can also be mutated from menu commands (zoom in/out) rather than
/// only from the settings window.
@MainActor
final class ReaderSettings: ObservableObject {
    static let minimumFontSize: Double = 11
    static let maximumFontSize: Double = 28
    static let defaultFontSize: Double = 15

    private enum Key {
        static let appearance = "mdstar.native.appearance"
        static let fontFamily = "mdstar.native.reader.fontFamily"
        static let fontSize = "mdstar.native.reader.fontSize"
        static let lineSpacing = "mdstar.native.reader.lineSpacing"
        static let contentWidth = "mdstar.native.reader.contentWidth"
    }

    @Published var appearance: AppearancePreference {
        didSet { UserDefaults.standard.set(appearance.rawValue, forKey: Key.appearance) }
    }

    @Published var fontFamily: ReaderFontFamily {
        didSet { UserDefaults.standard.set(fontFamily.rawValue, forKey: Key.fontFamily) }
    }

    /// Base body point size. Every other text style scales from this.
    @Published var fontSize: Double {
        didSet {
            let clamped = min(max(fontSize, Self.minimumFontSize), Self.maximumFontSize)
            if clamped != fontSize {
                fontSize = clamped
                return
            }
            UserDefaults.standard.set(fontSize, forKey: Key.fontSize)
        }
    }

    /// Extra leading added between lines of body text.
    @Published var lineSpacing: Double {
        didSet { UserDefaults.standard.set(lineSpacing, forKey: Key.lineSpacing) }
    }

    /// Maximum measure for prose, in points.
    @Published var contentWidth: Double {
        didSet { UserDefaults.standard.set(contentWidth, forKey: Key.contentWidth) }
    }

    init() {
        let defaults = UserDefaults.standard
        appearance = AppearancePreference(rawValue: defaults.string(forKey: Key.appearance) ?? "")
            ?? .system
        fontFamily = ReaderFontFamily(rawValue: defaults.string(forKey: Key.fontFamily) ?? "")
            ?? .system
        let storedSize = defaults.double(forKey: Key.fontSize)
        fontSize = storedSize > 0 ? storedSize : Self.defaultFontSize
        let storedSpacing = defaults.double(forKey: Key.lineSpacing)
        lineSpacing = storedSpacing > 0 ? storedSpacing : 4.5
        let storedWidth = defaults.double(forKey: Key.contentWidth)
        contentWidth = storedWidth > 0 ? storedWidth : 740
    }

    // MARK: - Zoom

    var canZoomIn: Bool { fontSize < Self.maximumFontSize }
    var canZoomOut: Bool { fontSize > Self.minimumFontSize }

    func zoomIn() { fontSize = min(fontSize + 1, Self.maximumFontSize) }
    func zoomOut() { fontSize = max(fontSize - 1, Self.minimumFontSize) }
    func resetZoom() { fontSize = Self.defaultFontSize }

    // MARK: - Derived fonts

    /// Body font at the current size and family.
    var bodyFont: Font {
        .system(size: fontSize, design: fontFamily.design)
    }

    /// Monospaced font for code, tracking the reader size but never the family.
    var codeFont: Font {
        .system(size: fontSize - 1.5, design: .monospaced)
    }

    /// Point size for a heading level. Exposed separately from `headingFont`
    /// because SwiftUI's `Font` is opaque and cannot be inspected in tests.
    func headingPointSize(level: Int) -> Double {
        let multiplier: Double
        switch level {
        case 1: multiplier = 2.05
        case 2: multiplier = 1.55
        case 3: multiplier = 1.28
        case 4: multiplier = 1.12
        case 5: multiplier = 1.0
        default: multiplier = 0.92
        }
        return fontSize * multiplier
    }

    /// Headings scale proportionally so the hierarchy holds at any zoom level.
    func headingFont(level: Int) -> Font {
        .system(
            size: headingPointSize(level: level),
            weight: level <= 1 ? .bold : .semibold,
            design: fontFamily.design
        )
    }

    /// Slightly smaller face for table cells and captions.
    var secondaryFont: Font {
        .system(size: fontSize - 1, design: fontFamily.design)
    }
}
