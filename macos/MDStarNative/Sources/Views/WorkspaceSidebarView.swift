import SwiftUI

struct WorkspaceSidebarView: View {
    @ObservedObject var workspace: WorkspaceStore
    @State private var search = ""

    private var isSearching: Bool { !search.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            searchField
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 3) {
                    SectionHeader(title: "Workspace", trailing: workspaceMenu)
                    workspaceContent
                    SectionHeader(title: "Symbols Structure")
                        .padding(.top, 10)
                    outlineContent
                }
                .padding(.horizontal, 10)
                .padding(.bottom, 14)
            }
        }
        .background(.regularMaterial)
    }

    // MARK: - Search

    private var searchField: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.callout)
            TextField("Search", text: $search)
                .textFieldStyle(.plain)
            if isSearching {
                Button { search = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 6)
        .background(Color.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 7))
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(Color.readerHairline))
        .padding(.horizontal, 10)
        .padding(.top, 54)
        .padding(.bottom, 8)
    }

    // MARK: - Workspace tree

    @ViewBuilder
    private var workspaceContent: some View {
        if let tree = workspace.fileTree, let filtered = filterNode(tree, query: search) {
            FileTreeView(root: filtered, isSearching: isSearching, workspace: workspace)
        } else if workspace.fileTree != nil {
            emptyHint("No files match “\(search)”.")
        } else {
            Button {
                workspace.chooseWorkspace()
            } label: {
                Label("Open a Folder…", systemImage: "folder.badge.plus")
                    .font(.callout)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .padding(.vertical, 4)
        }
    }

    @ViewBuilder
    private var workspaceMenu: some View {
        if workspace.workspaceURL != nil {
            Menu {
                Button("Change Folder…", action: workspace.chooseWorkspace)
                Button("Close Folder", role: .destructive, action: workspace.closeWorkspace)
            } label: {
                Image(systemName: "ellipsis")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
        }
    }

    // MARK: - Outline

    @ViewBuilder
    private var outlineContent: some View {
        let outline = filteredOutline
        if outline.isEmpty {
            emptyHint(workspace.document == nil ? "Open a document to see its structure." : "No headings found.")
        } else {
            ForEach(outline) { heading in
                OutlineRow(
                    heading: heading,
                    isActive: heading.id == workspace.activeHeadingID,
                    isBookmarked: workspace.isBookmarked(heading.id),
                    onSelect: { workspace.focus(blockID: heading.id) },
                    onToggleBookmark: { workspace.toggleBookmark(id: heading.id, title: heading.text, level: heading.level) }
                )
            }
        }
    }

    private var filteredOutline: [OutlineItem] {
        guard let outline = workspace.document?.outline else { return [] }
        guard isSearching else { return outline }
        let query = search.lowercased()
        return outline.filter { $0.text.lowercased().contains(query) }
    }

    private func emptyHint(_ text: String) -> some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(.tertiary)
            .padding(.vertical, 4)
            .padding(.horizontal, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Search filtering

    private func filterNode(_ node: FileNode, query: String) -> FileNode? {
        guard isSearching else { return node }
        let needle = query.lowercased()
        if node.isDirectory {
            let kids = (node.children ?? []).compactMap { filterNode($0, query: query) }
            if node.name.lowercased().contains(needle) || !kids.isEmpty {
                return FileNode(id: node.id, name: node.name, path: node.path, kind: node.kind, children: kids)
            }
            return nil
        }
        return node.name.lowercased().contains(needle) ? node : nil
    }
}

// MARK: - Section header

private struct SectionHeader<Trailing: View>: View {
    let title: String
    var trailing: Trailing

    var body: some View {
        HStack {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .tracking(0.6)
            Spacer()
            trailing
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 5)
    }
}

extension SectionHeader where Trailing == EmptyView {
    init(title: String) {
        self.init(title: title, trailing: EmptyView())
    }
}

// MARK: - Outline row

private struct OutlineRow: View {
    let heading: OutlineItem
    let isActive: Bool
    let isBookmarked: Bool
    let onSelect: () -> Void
    let onToggleBookmark: () -> Void

    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 8) {
            Text(heading.text)
                .font(.callout)
                .fontWeight(isActive ? .semibold : .regular)
                .foregroundStyle(isActive ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(heading.level == 1 ? .primary : .secondary))
                .lineLimit(1)
            Spacer(minLength: 0)
            if isBookmarked {
                Image(systemName: "bookmark.fill")
                    .font(.system(size: 9))
                    .foregroundStyle(Color.accentColor)
            }
        }
        .padding(.leading, CGFloat(max(heading.level - 1, 0)) * 14 + 8)
        .padding(.trailing, 8)
        .padding(.vertical, 4.5)
        .background(rowBackground, in: RoundedRectangle(cornerRadius: 6))
        .contentShape(RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering = $0 }
        .onTapGesture(perform: onSelect)
        .contextMenu {
            Button(isBookmarked ? "Remove Bookmark" : "Bookmark", systemImage: isBookmarked ? "bookmark.slash" : "bookmark") {
                onToggleBookmark()
            }
        }
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel("Heading level \(heading.level): \(heading.text)")
    }

    private var rowBackground: Color {
        if isActive { return Color.accentColor.opacity(0.10) }
        if isHovering { return Color.primary.opacity(0.05) }
        return .clear
    }
}
