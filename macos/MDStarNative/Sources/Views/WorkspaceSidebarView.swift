import SwiftUI

/// Native workspace source-list. Titlebar controls are supplied by the
/// parent window toolbar, not by the sidebar layout.
struct WorkspaceSidebarView: View {
    @ObservedObject var workspace: WorkspaceStore
    @State private var search = ""

    private var isSearching: Bool { !search.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        List {
            Section("Workspace") {
                workspaceControls
            }

            Section("Files") {
                workspaceContent
            }

            Section("Document Structure") {
                outlineContent
            }
        }
        .listStyle(.sidebar)
        .searchable(text: $search, placement: .sidebar, prompt: "Filter files and headings")
        .navigationTitle("MD Star")
    }

    @ViewBuilder
    private var workspaceControls: some View {
        if let workspaceURL = workspace.workspaceURL {
            Label(workspaceURL.lastPathComponent, systemImage: "folder")
                .lineLimit(1)
                .contextMenu {
                    Button("Change Folder…", action: workspace.chooseWorkspace)
                    Button("Close Folder", role: .destructive, action: workspace.closeWorkspace)
                }
        } else {
            Button(action: workspace.chooseWorkspace) {
                Label("Open Workspace…", systemImage: "folder.badge.plus")
            }
        }
    }

    @ViewBuilder
    private var workspaceContent: some View {
        if let tree = workspace.fileTree, let filtered = filterNode(tree, query: search) {
            FileTreeView(root: filtered, isSearching: isSearching, workspace: workspace)
        } else if workspace.fileTree != nil {
            SidebarHint("No files match “\(search)”.")
        } else {
            SidebarHint("Open a folder to browse its documents.")
        }
    }

    @ViewBuilder
    private var outlineContent: some View {
        let outline = filteredOutline
        if outline.isEmpty {
            SidebarHint(workspace.document == nil ? "Open a document to see its structure." : "No headings found.")
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

    private func filterNode(_ node: FileNode, query: String) -> FileNode? {
        guard isSearching else { return node }
        let needle = query.lowercased()
        if node.isDirectory {
            let children = (node.children ?? []).compactMap { filterNode($0, query: query) }
            if node.name.lowercased().contains(needle) || !children.isEmpty {
                return FileNode(id: node.id, name: node.name, path: node.path, kind: node.kind, children: children)
            }
            return nil
        }
        return node.name.lowercased().contains(needle) ? node : nil
    }
}

private struct SidebarHint: View {
    let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .foregroundStyle(.secondary)
            .font(.callout)
    }
}

private struct OutlineRow: View {
    let heading: OutlineItem
    let isActive: Bool
    let isBookmarked: Bool
    let onSelect: () -> Void
    let onToggleBookmark: () -> Void

    var body: some View {
        Button(action: onSelect) {
            HStack(spacing: 6) {
                Text(heading.text)
                    .fontWeight(isActive ? .semibold : .regular)
                    .lineLimit(1)
                Spacer(minLength: 0)
                if isBookmarked {
                    Image(systemName: "bookmark.fill")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.leading, CGFloat(max(heading.level - 1, 0)) * 12)
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button(isBookmarked ? "Remove Bookmark" : "Bookmark", systemImage: isBookmarked ? "bookmark.slash" : "bookmark") {
                onToggleBookmark()
            }
        }
        .accessibilityLabel("Heading level \(heading.level): \(heading.text)")
    }
}
