import SwiftUI

/// A Safari-like document strip hosted in the native principal toolbar item.
/// Each tab
/// carries a compact path breadcrumb so similarly named files remain distinct.
struct DocumentTabBar: View {
    let urls: [URL]
    let workspaceURL: URL?
    let selectedURL: URL?
    let select: (URL) -> Void
    let close: (URL) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                ForEach(urls, id: \.self) { url in
                    tab(for: url)
                }
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 30)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Open documents")
    }

    private func tab(for url: URL) -> some View {
        let isSelected = url == selectedURL
        return HStack(spacing: 4) {
            Button {
                select(url)
            } label: {
                tabLabel(for: url, selected: isSelected)
            }
            .buttonStyle(.plain)

            Button {
                close(url)
            } label: {
                Image(systemName: "xmark")
                    .font(.caption2.weight(.bold))
                    .frame(width: 15, height: 15)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(isSelected ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tertiary))
            .help("Close \(url.lastPathComponent)")
            .accessibilityLabel("Close \(url.lastPathComponent)")
        }
        .font(.subheadline)
        .padding(.leading, 9)
        .padding(.trailing, 7)
        .padding(.vertical, 5)
        .frame(minWidth: 168, idealWidth: 228, maxWidth: 276, alignment: .leading)
        .foregroundStyle(isSelected ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary))
        .background {
            tabBackground(selected: isSelected)
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        }
        .overlay {
            if isSelected {
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .stroke(Color.primary.opacity(0.10))
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        .contextMenu {
            Button("Close Tab") { close(url) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(accessibilityPath(for: url))
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }

    private func tabLabel(for url: URL, selected: Bool) -> some View {
        let components = breadcrumbComponents(for: url)
        return HStack(spacing: 4) {
            Image(systemName: "doc.text")
                .imageScale(.small)

            if components.count > 1 {
                Text(components.dropLast().joined(separator: " › "))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.tertiary)
            }

            Text(components.last ?? url.lastPathComponent)
                .fontWeight(selected ? .semibold : .regular)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func tabBackground(selected: Bool) -> some View {
        if selected {
            if #available(macOS 26.0, *) {
                Color.clear.glassEffect(.regular, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            } else {
                Color.primary.opacity(0.11)
            }
        } else {
            Color.primary.opacity(0.035)
        }
    }

    private func breadcrumbComponents(for url: URL) -> [String] {
        guard let workspaceURL else { return [url.lastPathComponent] }
        let root = workspaceURL.standardizedFileURL.path
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(root) else { return [url.lastPathComponent] }

        let relative = String(path.dropFirst(root.count))
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let all = relative.split(separator: "/").map(String.init)
        // Two parent levels retain useful location context without letting one
        // deeply nested tab starve every other tab in the scroll strip.
        return all.count > 3 ? Array(all.suffix(3)) : all
    }

    private func accessibilityPath(for url: URL) -> String {
        breadcrumbComponents(for: url).joined(separator: ", ")
    }
}
