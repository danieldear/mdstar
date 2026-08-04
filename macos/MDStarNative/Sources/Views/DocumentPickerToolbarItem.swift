import SwiftUI

/// Open-document switcher for the toolbar.
///
/// Replaces the tab strip: a tab bar inside an `NSToolbar` is not a macOS
/// pattern and fought the toolbar's own layout. This is the standard pop-up
/// treatment — the current document is always shown, and the menu lists the
/// rest with a close control on each row.
struct DocumentPickerToolbarItem: View {
    let urls: [URL]
    let workspaceURL: URL?
    let selectedURL: URL?
    let select: (URL) -> Void
    let close: (URL) -> Void

    var body: some View {
        Menu {
            Section("Open Documents") {
                ForEach(urls, id: \.self) { url in
                    documentRow(url)
                }
            }

            if urls.count > 1 {
                Divider()
                Button("Close Others") {
                    for url in urls where url != selectedURL { close(url) }
                }
            }
            if let selectedURL {
                Button("Close \u{201C}\(selectedURL.lastPathComponent)\u{201D}") {
                    close(selectedURL)
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "doc.text")
                    .foregroundStyle(.secondary)
                Text(title)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if urls.count > 1 {
                    Text("\(urls.count)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(Color.readerStrongFill, in: Capsule())
                }
            }
            .frame(minWidth: 120)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .disabled(urls.isEmpty)
        .help(helpText)
        .accessibilityLabel("Open documents")
    }

    /// Each row switches documents; the trailing button closes without switching.
    @ViewBuilder
    private func documentRow(_ url: URL) -> some View {
        Button {
            select(url)
        } label: {
            if url == selectedURL {
                Label(label(for: url), systemImage: "checkmark")
            } else {
                Text(label(for: url))
            }
        }
    }

    private var title: String {
        selectedURL?.lastPathComponent ?? "No Document"
    }

    private var helpText: String {
        urls.count > 1 ? "Switch between \(urls.count) open documents" : "Open documents"
    }

    /// Disambiguates identical file names by including the parent directory.
    private func label(for url: URL) -> String {
        let name = url.lastPathComponent
        let duplicated = urls.filter { $0.lastPathComponent == name }.count > 1
        guard duplicated else { return name }
        let parent = url.deletingLastPathComponent().lastPathComponent
        return parent.isEmpty ? name : "\(parent)/\(name)"
    }
}
