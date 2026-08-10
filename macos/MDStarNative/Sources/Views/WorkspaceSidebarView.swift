import AppKit
import SwiftUI

/// Native workspace source-list. Titlebar controls are supplied by the
/// parent window toolbar, not by the sidebar layout.
struct WorkspaceSidebarView: View {
    @ObservedObject var workspace: WorkspaceStore
    @State private var search = ""

    private var isSearching: Bool { !search.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            // `.searchable(placement: .sidebar)` only remains sidebar-local
            // while this view is hosted by NavigationSplitView. The workspace
            // now uses an outer HSplitView so its three columns cannot overlap;
            // keep the native search control physically inside this column
            // rather than allowing SwiftUI to promote it into the window
            // toolbar.
            SidebarSearchField(
                text: $search,
                prompt: "Filter files and headings"
            )
            .frame(height: 28)
            .padding(.horizontal, 12)
            .padding(.top, 8)
            .padding(.bottom, 6)

            List {
                Section {
                    folderRoots
                } header: {
                    HStack {
                        Text("Folders")
                        Spacer()
                        Button(action: workspace.chooseWorkspace) {
                            Image(systemName: "plus")
                        }
                        .buttonStyle(.borderless)
                        .help("Add Folder to Workspace")
                        .accessibilityLabel("Add folder to workspace")
                    }
                }

                Section("Document Structure") {
                    outlineContent
                }
            }
            .listStyle(.sidebar)
        }
    }

    // MARK: - Folders

    @ViewBuilder
    private var folderRoots: some View {
        if workspace.folders.isEmpty {
            Button(action: workspace.chooseWorkspace) {
                Label("Add Folder…", systemImage: "folder.badge.plus")
            }
        } else {
            ForEach(workspace.folders) { folder in
                FolderRootView(
                    folder: folder,
                    filter: search,
                    isSearching: isSearching,
                    workspace: workspace
                )
            }
        }
    }

    // MARK: - Outline

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
                    onToggleBookmark: {
                        workspace.toggleBookmark(id: heading.id, title: heading.text, level: heading.level)
                    }
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
}

/// A real `NSSearchField` keeps the standard macOS search affordance, clear
/// button, keyboard behaviour, and accessibility while remaining part of the
/// sidebar's layout instead of becoming a window-toolbar item.
private struct SidebarSearchField: NSViewRepresentable {
    @Binding var text: String
    let prompt: String

    func makeNSView(context: Context) -> NSSearchField {
        let field = NSSearchField()
        field.placeholderString = prompt
        field.sendsSearchStringImmediately = true
        field.sendsWholeSearchString = false
        field.delegate = context.coordinator
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        if field.stringValue != text {
            field.stringValue = text
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        private var text: Binding<String>

        init(text: Binding<String>) {
            self.text = text
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSSearchField else { return }
            text.wrappedValue = field.stringValue
        }
    }
}

// MARK: - Folder root

private struct FolderRootView: View {
    let folder: WorkspaceFolder
    let filter: String
    let isSearching: Bool
    @ObservedObject var workspace: WorkspaceStore

    var body: some View {
        DisclosureGroup {
            if folder.isUnavailable {
                SidebarHint("This folder is no longer available.")
            } else if let tree = folder.tree, let filtered = filterNode(tree) {
                FileTreeView(root: filtered, isSearching: isSearching, workspace: workspace)
            } else if folder.tree != nil {
                SidebarHint("No files match \u{201C}\(filter)\u{201D}.")
            } else {
                SidebarHint("Reading folder\u{2026}")
            }
        } label: {
            Label {
                Text(folder.name).lineLimit(1)
            } icon: {
                Image(systemName: folder.isUnavailable ? "folder.badge.questionmark" : "folder")
                    .foregroundStyle(folder.isUnavailable ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
            }
        }
        .contextMenu {
            Button("Reveal in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([folder.url])
            }
            Button("Refresh") { workspace.refreshFolders() }
            Divider()
            Button("Remove from Workspace", role: .destructive) {
                workspace.removeFolder(folder)
            }
        }
        .accessibilityLabel("Workspace folder \(folder.name)")
    }

    private func filterNode(_ node: FileNode) -> FileNode? {
        guard isSearching else { return node }
        let needle = filter.lowercased()
        if node.isDirectory {
            let children = (node.children ?? []).compactMap { filterNode($0) }
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

// MARK: - Outline row

private struct OutlineRow: View {
    let heading: OutlineItem
    let isActive: Bool
    let isBookmarked: Bool
    let onSelect: () -> Void
    let onToggleBookmark: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: onSelect) {
            HStack(spacing: 6) {
                Text(heading.text)
                    .fontWeight(isActive ? .semibold : .regular)
                    .lineLimit(1)
                Spacer(minLength: 0)
                // The bookmark control stays visible when set and appears on
                // hover otherwise — a context menu alone left the feature
                // effectively undiscoverable.
                if isBookmarked || isHovering {
                    Button(action: onToggleBookmark) {
                        Image(systemName: isBookmarked ? "bookmark.fill" : "bookmark")
                            .font(.caption2)
                            .foregroundStyle(isBookmarked ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.tertiary))
                    }
                    .buttonStyle(.borderless)
                    .help(isBookmarked ? "Remove Bookmark" : "Bookmark This Section")
                    .accessibilityLabel(isBookmarked ? "Remove bookmark" : "Add bookmark")
                }
            }
            .padding(.leading, CGFloat(max(heading.level - 1, 0)) * 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .contextMenu {
            Button(isBookmarked ? "Remove Bookmark" : "Bookmark", systemImage: isBookmarked ? "bookmark.slash" : "bookmark") {
                onToggleBookmark()
            }
        }
        .accessibilityLabel("Heading level \(heading.level): \(heading.text)")
    }
}
