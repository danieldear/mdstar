import SwiftUI

/// Central design tokens for the native reader. Values favor macOS defaults and
/// the user's system accent color rather than a fixed brand palette.
enum Theme {
    /// Ideal measure for long-form reading. Body text is capped to this width
    /// and centered; full-bleed blocks (tables, code) may exceed it.
    static let readerContentWidth: CGFloat = 740

    /// Horizontal and vertical padding around the reader column.
    static let readerHorizontalPadding: CGFloat = 40
    static let readerVerticalPadding: CGFloat = 36

    /// Vertical rhythm between top-level blocks.
    static let blockSpacing: CGFloat = 18

    /// Corner radius shared by cards, code blocks and tables.
    static let cornerRadius: CGFloat = 10

    enum HeadingScale {
        static func font(for level: Int) -> Font {
            switch level {
            case 1: .system(.largeTitle, design: .default, weight: .bold)
            case 2: .system(.title, design: .default, weight: .semibold)
            case 3: .system(.title2, design: .default, weight: .semibold)
            case 4: .system(.title3, design: .default, weight: .semibold)
            case 5: .system(.headline, design: .default)
            default: .system(.subheadline, design: .default, weight: .semibold)
            }
        }

        static func topPadding(for level: Int) -> CGFloat {
            switch level {
            case 1: 8
            case 2: 22
            case 3: 16
            default: 10
            }
        }
    }
}

extension Color {
    /// Subtle fill used behind inline code, table headers and code blocks.
    static var readerSubtleFill: Color { Color.primary.opacity(0.06) }
    static var readerStrongFill: Color { Color.primary.opacity(0.09) }
    static var readerHairline: Color { Color.primary.opacity(0.12) }
}


extension View {
    /// Uses the system Liquid Glass material when it is available, with a
    /// compact material fallback for earlier supported macOS versions.
    @ViewBuilder
    func floatingGlass<S: Shape>(_ shape: S) -> some View {
        if #available(macOS 26.0, *) {
            self.glassEffect(.regular, in: shape)
        } else {
            self
                .background(.regularMaterial, in: shape)
                .overlay(shape.stroke(Color.readerHairline))
                .shadow(color: .black.opacity(0.12), radius: 8, y: 3)
        }
    }
}
