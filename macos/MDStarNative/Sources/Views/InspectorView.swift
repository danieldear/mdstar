import SwiftUI

struct InspectorView: View {
    @ObservedObject var inspector: InspectorStore
    @ObservedObject var workspace: WorkspaceStore

    var body: some View {
        VStack(spacing: 0) {
            Picker("Inspector", selection: $inspector.selectedSection) {
                ForEach(InspectorSection.allCases) { section in
                    Image(systemName: section.systemImage).tag(section)
                }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .padding(12)

            Divider()

            Group {
                switch inspector.selectedSection {
                case .bookmarks: bookmarks
                case .highlights: comingSoon(.highlights)
                case .comments: comingSoon(.comments)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityLabel("Document inspector")
    }

    // MARK: - Bookmarks

    @ViewBuilder
    private var bookmarks: some View {
        if workspace.bookmarks.isEmpty {
            emptyState(
                title: "No Bookmarks",
                systemImage: "bookmark",
                message: "Right-click a heading in the Symbols Structure list and choose “Bookmark” to save your place."
            )
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(workspace.bookmarks) { bookmark in
                        BookmarkRow(
                            bookmark: bookmark,
                            isActive: bookmark.id == workspace.activeHeadingID,
                            onSelect: { workspace.focus(blockID: bookmark.id) },
                            onRemove: { workspace.removeBookmark(bookmark.id) }
                        )
                    }
                }
                .padding(10)
            }
        }
    }

    // MARK: - Not-yet-available sections

    @ViewBuilder
    private func comingSoon(_ section: InspectorSection) -> some View {
        emptyState(
            title: section.rawValue,
            systemImage: section.systemImage,
            message: "\(section.rawValue) attach to a text selection. They arrive with the upcoming rich-text reader that tracks selected ranges."
        )
    }

    private func emptyState(title: String, systemImage: String, message: String) -> some View {
        VStack(spacing: 10) {
            Image(systemName: systemImage)
                .font(.system(size: 26))
                .foregroundStyle(.tertiary)
                .symbolRenderingMode(.hierarchical)
            Text(title).font(.headline)
            Text(message)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct BookmarkRow: View {
    let bookmark: Bookmark
    let isActive: Bool
    let onSelect: () -> Void
    let onRemove: () -> Void

    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "bookmark.fill")
                .font(.caption)
                .foregroundStyle(Color.accentColor)
            Text(bookmark.title)
                .lineLimit(1)
                .foregroundStyle(isActive ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.primary))
            Spacer(minLength: 0)
            if isHovering {
                Button(action: onRemove) { Image(systemName: "xmark.circle.fill") }
                    .buttonStyle(.plain)
                    .foregroundStyle(.tertiary)
                    .help("Remove bookmark")
            }
        }
        .font(.callout)
        .padding(.leading, CGFloat(max(bookmark.level - 1, 0)) * 12 + 8)
        .padding(.trailing, 8)
        .padding(.vertical, 5)
        .background(isHovering ? Color.primary.opacity(0.05) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .contentShape(RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering = $0 }
        .onTapGesture(perform: onSelect)
    }
}
