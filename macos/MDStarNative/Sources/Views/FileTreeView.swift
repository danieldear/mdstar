import SwiftUI

/// Recursive, native DisclosureGroup-based workspace tree. Expansion remains
/// persisted by WorkspaceStore while row spacing and selection stay source-list
/// appropriate.
struct FileTreeView: View {
    let root: FileNode
    let isSearching: Bool
    @ObservedObject var workspace: WorkspaceStore

    var body: some View {
        ForEach(root.children ?? []) { node in
            FileTreeNodeView(node: node, isSearching: isSearching, workspace: workspace)
        }
    }
}

private struct FileTreeNodeView: View {
    let node: FileNode
    let isSearching: Bool
    @ObservedObject var workspace: WorkspaceStore

    var body: some View {
        if node.isDirectory {
            DisclosureGroup(isExpanded: expandedBinding) {
                ForEach(node.children ?? []) { child in
                    FileTreeNodeView(node: child, isSearching: isSearching, workspace: workspace)
                }
            } label: {
                Label(node.name, systemImage: "folder")
                    .lineLimit(1)
            }
        } else {
            Button {
                workspace.openFile(node.url, recordHistory: true)
            } label: {
                Label(node.name, systemImage: "doc.text")
                    .lineLimit(1)
                    .foregroundStyle(node.path == workspace.selectedURL?.path ? Color.accentColor : Color.primary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Open \(node.name)")
        }
    }

    private var expandedBinding: Binding<Bool> {
        Binding(
            get: { isSearching || workspace.isExpanded(node.id) },
            set: { expanded in
                guard !isSearching else { return }
                workspace.setExpanded(node.id, expanded: expanded)
            }
        )
    }
}
