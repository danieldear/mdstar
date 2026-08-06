import SwiftUI

struct InspectorView: View {
    @ObservedObject var inspector: InspectorStore
    @ObservedObject var workspace: WorkspaceStore
    @ObservedObject var annotations: AnnotationStore

    private var documentID: String? { workspace.document?.documentID }

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
                case .highlights: annotationList(kind: .highlight)
                case .comments: annotationList(kind: .comment)
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
                message: "Hover a heading in Document Structure and click the bookmark control to save your place."
            )
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(workspace.bookmarks) { bookmark in
                        InspectorRow(
                            icon: "bookmark.fill",
                            tint: .accentColor,
                            title: bookmark.title,
                            subtitle: nil,
                            isActive: bookmark.id == workspace.activeHeadingID,
                            indent: CGFloat(max(bookmark.level - 1, 0)) * 12,
                            onSelect: { workspace.focus(blockID: bookmark.id) },
                            onRemove: { workspace.removeBookmark(bookmark.id) }
                        )
                    }
                }
                .padding(10)
            }
        }
    }

    // MARK: - Highlights and comments

    @ViewBuilder
    private func annotationList(kind: Annotation.Kind) -> some View {
        let items = documentID.map { annotations.annotations(for: $0, kind: kind) } ?? []
        if items.isEmpty {
            emptyState(
                title: kind == .highlight ? "No Highlights" : "No Comments",
                systemImage: kind.systemImage,
                message: kind == .highlight
                    ? "Select text in the document, then right-click and choose Highlight."
                    : "Select text in the document, then right-click and choose Add Comment."
            )
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(items) { annotation in
                        InspectorRow(
                            icon: kind.systemImage,
                            tint: kind == .highlight ? .yellow : .blue,
                            title: annotation.snippet,
                            subtitle: annotation.note.isEmpty ? nil : annotation.note,
                            isActive: false,
                            indent: 0,
                            onSelect: { reveal(annotation) },
                            onRemove: {
                                if let documentID {
                                    annotations.remove(annotation.id, documentID: documentID)
                                }
                            }
                        )
                    }
                }
                .padding(10)
            }
        }
    }

    /// Scrolls the reader to the annotated passage.
    private func reveal(_ annotation: Annotation) {
        guard let blockID = annotation.segments?.first?.blockID ?? annotation.blockID else { return }
        workspace.focus(blockID: blockID)
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

private struct InspectorRow: View {
    let icon: String
    let tint: Color
    let title: String
    let subtitle: String?
    let isActive: Bool
    let indent: CGFloat
    let onSelect: () -> Void
    let onRemove: () -> Void

    @State private var isHovering = false

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon)
                .font(.caption)
                .foregroundStyle(tint)
                .padding(.top, 2)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .lineLimit(2)
                    .foregroundStyle(isActive ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.primary))
                if let subtitle {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                }
            }

            Spacer(minLength: 0)

            if isHovering {
                Button(action: onRemove) {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tertiary)
                .help("Remove")
            }
        }
        .font(.callout)
        .padding(.leading, indent + 8)
        .padding(.trailing, 8)
        .padding(.vertical, 5)
        .background(isHovering ? Color.primary.opacity(0.05) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .contentShape(RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering = $0 }
        .onTapGesture(perform: onSelect)
    }
}
