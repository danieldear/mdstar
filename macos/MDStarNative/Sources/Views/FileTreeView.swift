import SwiftUI

struct FileTreeView: View {
    let root: FileNode
    let isSearching: Bool
    @ObservedObject var workspace: WorkspaceStore

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            ForEach(root.children ?? []) { node in
                FileTreeNodeView(node: node, depth: 0, isSearching: isSearching, workspace: workspace)
            }
        }
    }
}

private struct FileTreeNodeView: View {
    let node: FileNode
    let depth: Int
    let isSearching: Bool
    @ObservedObject var workspace: WorkspaceStore

    var body: some View {
        if node.isDirectory {
            VStack(alignment: .leading, spacing: 1) {
                SidebarRow(
                    title: node.name,
                    systemImage: isExpanded ? "folder" : "folder.fill",
                    depth: depth,
                    isSelected: false,
                    accessory: Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.tertiary)
                )
                .contentShape(Rectangle())
                .onTapGesture { toggle() }

                if isExpanded {
                    ForEach(node.children ?? []) { child in
                        FileTreeNodeView(node: child, depth: depth + 1, isSearching: isSearching, workspace: workspace)
                    }
                }
            }
        } else {
            SidebarRow(
                title: node.name,
                systemImage: "doc.text",
                depth: depth,
                isSelected: node.path == workspace.selectedURL?.path
            )
            .contentShape(Rectangle())
            .onTapGesture { workspace.openFile(node.url, recordHistory: true) }
            .accessibilityAddTraits(.isButton)
        }
    }

    private var isExpanded: Bool {
        isSearching || workspace.isExpanded(node.id)
    }

    private func toggle() {
        guard !isSearching else { return }
        workspace.setExpanded(node.id, expanded: !workspace.isExpanded(node.id))
    }
}

/// Shared sidebar row: consistent height, indentation, hover, and selection
/// treatment that uses the system accent color.
struct SidebarRow<Accessory: View>: View {
    let title: String
    let systemImage: String
    var depth: Int = 0
    var isSelected: Bool = false
    var accessory: Accessory

    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: systemImage)
                .font(.callout)
                .foregroundStyle(isSelected ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.secondary))
                .frame(width: 18)
            Text(title)
                .lineLimit(1)
                .foregroundStyle(isSelected ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.primary))
            Spacer(minLength: 0)
            accessory
        }
        .font(.callout)
        .padding(.leading, CGFloat(depth) * 14 + 6)
        .padding(.trailing, 8)
        .padding(.vertical, 5)
        .background(rowBackground, in: RoundedRectangle(cornerRadius: 6))
        .contentShape(RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering = $0 }
    }

    private var rowBackground: Color {
        if isSelected { return Color.accentColor.opacity(0.14) }
        if isHovering { return Color.primary.opacity(0.05) }
        return .clear
    }
}

extension SidebarRow where Accessory == EmptyView {
    init(title: String, systemImage: String, depth: Int = 0, isSelected: Bool = false) {
        self.init(title: title, systemImage: systemImage, depth: depth, isSelected: isSelected, accessory: EmptyView())
    }
}
