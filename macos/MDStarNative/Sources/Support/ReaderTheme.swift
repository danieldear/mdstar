import AppKit

/// Builds the CSS that themes the Rust stylesheet.
///
/// The core ships structure with every colour and metric as a custom property;
/// this only redefines those variables from the macOS appearance, the system
/// accent colour, and the user's reading preferences. Keeping it to variables
/// means the frontend never restates layout rules.
@MainActor
enum ReaderTheme {
    static func overrides(settings: ReaderSettings, isDark: Bool) -> String {
        let accent = hex(NSColor.controlAccentColor)

        let text = isDark ? "#f2f2f5" : "#1c1c1e"
        let secondary = isDark ? "#a2a2a9" : "#6b6b72"
        let tertiary = isDark ? "#75757c" : "#97979e"
        let hairline = isDark ? "rgba(255,255,255,0.13)" : "rgba(0,0,0,0.11)"
        let fill = isDark ? "rgba(255,255,255,0.055)" : "rgba(0,0,0,0.045)"
        let fillStrong = isDark ? "rgba(255,255,255,0.09)" : "rgba(0,0,0,0.075)"
        let codeBackground = isDark ? "rgba(255,255,255,0.07)" : "rgba(0,0,0,0.045)"
        let pageBackground = isDark ? "#1e1e20" : "#ffffff"
        // Slightly lifted so the band beneath the toolbar is perceptible
        // without reading as a separate surface.
        let pageBackgroundTop = isDark ? "#282829" : "#f1f1f3"
        let mark = isDark ? "rgba(255,214,10,0.34)" : "rgba(255,214,10,0.45)"
        let markActive = isDark ? "rgba(255,159,10,0.72)" : "rgba(255,149,0,0.75)"

        // Syntax colours track the two appearances rather than being tinted.
        let comment = isDark ? "#7fd18c" : "#3b7a2e"
        let string = isDark ? "#ff8f7a" : "#c4351f"
        let number = isDark ? "#b39bff" : "#1c00cf"
        let keyword = isDark ? "#ff7ab6" : "#a3268e"

        return """
        :root {
          /* Pins native controls (checkboxes) to the reader's theme. Leaving
             this to the OS draws light-mode controls on a dark page whenever
             the reader's appearance is overridden. */
          color-scheme: \(isDark ? "dark" : "light");
          --reader-font-family: \(cssFontStack(for: settings.fontFamily));
          --reader-font-size: \(Int(settings.fontSize))px;
          --reader-line-height: \(lineHeight(for: settings));
          --reader-measure: \(Int(settings.contentWidth))px;

          --reader-text: \(text);
          --reader-secondary: \(secondary);
          --reader-tertiary: \(tertiary);
          --reader-accent: \(accent);
          --reader-hairline: \(hairline);
          --reader-fill: \(fill);
          --reader-fill-strong: \(fillStrong);
          --reader-code-bg: \(codeBackground);
          --reader-bg: \(pageBackground);
          --reader-bg-top: \(pageBackgroundTop);
          --reader-mark: \(mark);
          --reader-mark-active: \(markActive);

          --reader-syntax-comment: \(comment);
          --reader-syntax-string: \(string);
          --reader-syntax-number: \(number);
          --reader-syntax-keyword: \(keyword);
        }
        """
    }

    /// Line height is stored as extra leading in points; CSS wants a ratio.
    private static func lineHeight(for settings: ReaderSettings) -> String {
        let ratio = 1.0 + (settings.lineSpacing / max(settings.fontSize, 1)) + 0.32
        return String(format: "%.3f", min(max(ratio, 1.2), 2.4))
    }

    private static func cssFontStack(for family: ReaderFontFamily) -> String {
        switch family {
        case .system:
            return "-apple-system, BlinkMacSystemFont, \"SF Pro Text\", system-ui, sans-serif"
        case .serif:
            return "\"New York\", ui-serif, Georgia, Charter, serif"
        case .rounded:
            return "ui-rounded, \"SF Pro Rounded\", -apple-system, system-ui, sans-serif"
        case .monospaced:
            return "ui-monospace, \"SF Mono\", Menlo, monospace"
        }
    }

    private static func hex(_ color: NSColor) -> String {
        let converted = color.usingColorSpace(.sRGB) ?? color
        let red = Int((converted.redComponent * 255).rounded())
        let green = Int((converted.greenComponent * 255).rounded())
        let blue = Int((converted.blueComponent * 255).rounded())
        return String(format: "#%02x%02x%02x", red, green, blue)
    }
}
