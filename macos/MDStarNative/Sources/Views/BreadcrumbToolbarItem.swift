import SwiftUI

struct BreadcrumbToolbarItem: View {
    let workspaceURL: URL?
    let fileURL: URL?

    var body: some View {
        HStack(spacing: 6) {
            if let fileURL {
                let components = relativeComponents(for: fileURL)
                ForEach(components.indices, id: \.self) { index in
                    let isLast = index == components.count - 1
                    Text(components[index])
                        .foregroundStyle(isLast ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary))
                        .fontWeight(isLast ? .medium : .regular)
                        .lineLimit(1)
                        .truncationMode(isLast ? .middle : .tail)
                    if !isLast {
                        Image(systemName: "chevron.right")
                            .foregroundStyle(.tertiary)
                            .imageScale(.small)
                    }
                }
            } else {
                Text("No document open").foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 16)
        .font(.subheadline)
        .lineLimit(1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Current document path")
    }

    private func relativeComponents(for fileURL: URL) -> [String] {
        guard let workspaceURL else { return [fileURL.lastPathComponent] }
        let workspacePath = workspaceURL.standardizedFileURL.path
        let path = fileURL.standardizedFileURL.path
        guard path.hasPrefix(workspacePath) else { return [fileURL.lastPathComponent] }
        let relative = String(path.dropFirst(workspacePath.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let parts = relative.split(separator: "/").map(String.init)
        return parts.isEmpty ? [fileURL.lastPathComponent] : parts
    }
}
