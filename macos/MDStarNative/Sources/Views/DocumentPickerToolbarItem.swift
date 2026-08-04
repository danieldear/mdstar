import SwiftUI

/// Compact switcher for the open documents, shown beside the breadcrumb.
///
/// Replaces the tab strip: a tab bar inside an `NSToolbar` is not a macOS
/// pattern and fought the toolbar's own layout. Deliberately icon-only — the
/// breadcrumb already names the current document, so repeating it here would
/// duplicate information and crowd the title area.
struct DocumentPickerToolbarItem: View {
    let urls: [URL]
    let selectedURL: URL?
    let select: (URL) -> Void
    let close: (URL) -> Void

    var body: some View {
        Menu {
            Section("Open Documents") {
                ForEach(urls, id: \.self) { url in
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
            }

            Divider()

            if let selectedURL {
                Button("Close \u{201C}\(selectedURL.lastPathComponent)\u{201D}") {
                    close(selectedURL)
                }
            }
            if urls.count > 1 {
                Button("Close Others") {
                    for url in urls where url != selectedURL { close(url) }
                }
            }
        } label: {
            HStack(spacing: 3) {
                Text("\(urls.count)")
                    .font(.caption.monospacedDigit())
                Image(systemName: "chevron.down")
                    .font(.caption2.weight(.semibold))
            }
            .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Switch between \(urls.count) open documents")
        .accessibilityLabel("Open documents")
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
