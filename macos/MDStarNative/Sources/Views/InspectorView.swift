import SwiftUI

struct InspectorView: View {
    @ObservedObject var inspector: InspectorStore
    @ObservedObject var workspace: WorkspaceStore
    @ObservedObject var annotations: AnnotationStore

    private var documentID: String? { workspace.document?.documentID }

    var body: some View {
        VStack(spacing: 0) {
            inspectorSectionButtons
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

    /// A `Picker` using `.segmented` leaves AppKit in charge of its intrinsic
    /// sizing, so modifiers on its label do not produce dependable hit areas.
    /// Real buttons give every inspector section a consistent 32 pt target and
    /// make the selected state explicit.
    private var inspectorSectionButtons: some View {
        HStack(spacing: 8) {
            ForEach(InspectorSection.allCases, id: \.self) { section in
                inspectorSectionButton(section)

                if section != InspectorSection.allCases.last {
                    Divider()
                        .frame(height: 20)
                }
            }
        }
        .padding(4)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Inspector sections")
    }

    private func inspectorSectionButton(_ section: InspectorSection) -> some View {
        let isSelected = inspector.selectedSection == section

        return Button {
            inspector.selectedSection = section
        } label: {
            Image(systemName: section.systemImage)
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(isSelected ? Color.white : Color.primary)
        .background(
            isSelected ? Color.accentColor : Color.clear,
            in: RoundedRectangle(cornerRadius: 7, style: .continuous)
        )
        .help(section.rawValue)
        .accessibilityLabel(section.rawValue)
        .accessibilityValue(isSelected ? "Selected" : "")
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
                        if kind == .comment {
                            CommentInspectorRow(
                                annotation: annotation,
                                onReveal: { reveal(annotation) },
                                onRemove: {
                                    if let documentID {
                                        annotations.remove(annotation.id, documentID: documentID)
                                    }
                                }
                            )
                        } else {
                            InspectorRow(
                                icon: kind.systemImage,
                                tint: .yellow,
                                title: annotation.snippet,
                                subtitle: nil,
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
                }
                .padding(10)
            }
        }
    }

    /// Scrolls the reader to the annotated passage.
    private func reveal(_ annotation: Annotation) {
        guard let blockID = annotation.blockID else { return }
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

/// Comment rows open their content instead of silently navigating. Navigation
/// remains an explicit action inside the detail popover so reading a comment
/// never unexpectedly moves the document.
private struct CommentInspectorRow: View {
    let annotation: Annotation
    let onReveal: () -> Void
    let onRemove: () -> Void

    @State private var isDetailPresented = false

    var body: some View {
        InspectorRow(
            icon: Annotation.Kind.comment.systemImage,
            tint: .blue,
            title: annotation.note.isEmpty ? "Comment" : annotation.note,
            subtitle: annotation.snippet,
            isActive: false,
            indent: 0,
            onSelect: { isDetailPresented = true },
            onRemove: onRemove
        )
        .help("Open Comment")
        .popover(isPresented: $isDetailPresented, arrowEdge: .trailing) {
            CommentDetailPopover(
                annotation: annotation,
                onReveal: {
                    isDetailPresented = false
                    onReveal()
                },
                onDelete: {
                    isDetailPresented = false
                    onRemove()
                },
                onDismiss: { isDetailPresented = false }
            )
        }
    }
}

private struct CommentDetailPopover: View {
    let annotation: Annotation
    let onReveal: () -> Void
    let onDelete: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Label("Comment", systemImage: "text.bubble.fill")
                    .font(.headline)
                    .foregroundStyle(.blue)

                Spacer(minLength: 16)

                Text(annotation.createdAt, format: .dateTime.month(.abbreviated).day().year())
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(14)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Comment")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .textCase(.uppercase)

                        Text(annotation.note.isEmpty ? "No comment text." : annotation.note)
                            .font(.body)
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    VStack(alignment: .leading, spacing: 6) {
                        Text("Selected Passage")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .textCase(.uppercase)

                        Text(annotation.snippet)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(10)
                            .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
                    }
                }
                .padding(14)
            }
            .frame(maxHeight: 360)

            Divider()

            HStack {
                Button("Delete", role: .destructive, action: onDelete)

                Spacer()

                Button("Done", action: onDismiss)
                    .keyboardShortcut(.cancelAction)

                Button("Reveal in Document", action: onReveal)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(12)
        }
        .frame(width: 360)
        .accessibilityLabel("Comment details")
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
