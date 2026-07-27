import SwiftUI

/// Read-only Markdown source with a line-number gutter. A single `Text` holds
/// the code so selection works across lines; the gutter mirrors it line-for-line
/// because both use the same monospaced metrics and never wrap.
struct SourceTextView: View {
    let text: String

    private var lineCount: Int {
        max(text.reduce(into: 1) { count, character in if character == "\n" { count += 1 } }, 1)
    }

    private var gutterWidth: CGFloat {
        CGFloat(max(2, String(lineCount).count)) * 9 + 12
    }

    var body: some View {
        ScrollView([.vertical, .horizontal]) {
            HStack(alignment: .top, spacing: 14) {
                Text((1...lineCount).map(String.init).joined(separator: "\n"))
                    .font(.system(.callout, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.trailing)
                    .frame(width: gutterWidth, alignment: .trailing)

                Text(text.isEmpty ? " " : text)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .padding(.horizontal, 22)
            .padding(.top, Theme.floatingToolbarInset)
            .padding(.bottom, 18)
            .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
        }
        .background(Color(nsColor: .textBackgroundColor))
    }
}
