import AppKit
import SwiftUI

struct CodeBlockView: View {
    let language: String?
    let code: String
    @ObservedObject var settings: ReaderSettings

    @State private var didCopy = false
    @State private var isHovering = false

    private var hasLanguage: Bool { language?.isEmpty == false }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if hasLanguage || isHovering {
                header
            }
            ScrollView(.horizontal, showsIndicators: false) {
                Text(code.isEmpty ? AttributedString(" ") : SyntaxHighlighter.highlight(code, language: language))
                    .font(settings.codeFont)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 13)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.readerSubtleFill)
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadius))
        .onHover { isHovering = $0 }
    }

    private var header: some View {
        HStack(spacing: 8) {
            if hasLanguage {
                Text(language!.lowercased())
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if isHovering {
                Button(action: copy) {
                    Label(didCopy ? "Copied" : "Copy", systemImage: didCopy ? "checkmark" : "doc.on.doc")
                        .font(.caption)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 30)
        .animation(.easeInOut(duration: 0.12), value: isHovering)
    }

    private func copy() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(code, forType: .string)
        withAnimation { didCopy = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) {
            withAnimation { didCopy = false }
        }
    }
}
