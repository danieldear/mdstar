import AppKit
import SwiftUI

/// Reports the vertical position of each heading inside the reader scroll view
/// so the sidebar outline can highlight the section currently in view.
private struct HeadingAnchor: Equatable {
    let id: String
    let minY: CGFloat
}

private struct HeadingVisibilityKey: PreferenceKey {
    static let defaultValue: [HeadingAnchor] = []
    static func reduce(value: inout [HeadingAnchor], nextValue: () -> [HeadingAnchor]) {
        value.append(contentsOf: nextValue())
    }
}

struct DocumentRenderer: View {
    let document: DocumentIR
    @ObservedObject var settings: ReaderSettings
    let focusedBlockID: String?
    var searchQuery: String = ""
    var currentMatchID: String?
    let onOpenLink: (URL) -> Void
    var onActiveHeadingChange: (String?) -> Void = { _ in }

    private let scrollSpace = "readerScroll"

    var body: some View {
        ScrollViewReader { scrollProxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Theme.blockSpacing) {
                    ForEach(document.blocks) { block in
                        blockView(block, topLevel: true)
                            .frame(maxWidth: contentWidth(for: block), alignment: .leading)
                            .id(block.id)
                    }

                    if !errorDiagnostics.isEmpty {
                        DiagnosticsView(diagnostics: errorDiagnostics)
                            .frame(maxWidth: CGFloat(settings.contentWidth), alignment: .leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.horizontal, Theme.readerHorizontalPadding)
                .padding(.top, Theme.readerVerticalPadding)
                .padding(.bottom, Theme.readerVerticalPadding + 40)
            }
            .coordinateSpace(name: scrollSpace)
            .textSelection(.enabled)
            .environment(\.openURL, OpenURLAction { url in
                handleLink(url, scrollProxy: scrollProxy)
                return .handled
            })
            .onPreferenceChange(HeadingVisibilityKey.self) { anchors in
                updateActiveHeading(from: anchors)
            }
            .onChange(of: focusedBlockID) { blockID in
                guard let blockID else { return }
                withAnimation(.easeInOut(duration: 0.25)) {
                    scrollProxy.scrollTo(blockID, anchor: .top)
                }
            }
        }
    }

    private func handleLink(_ url: URL, scrollProxy: ScrollViewProxy) {
        if let slug = InlineRenderer.anchorSlug(from: url) {
            guard let targetID = anchorTargetID(for: slug) else { return }
            withAnimation(.easeInOut(duration: 0.25)) {
                scrollProxy.scrollTo(targetID, anchor: .top)
            }
            onActiveHeadingChange(targetID)
        } else {
            onOpenLink(url)
        }
    }

    private func anchorTargetID(for slug: String) -> String? {
        if let exact = document.outline.first(where: { $0.anchor == slug }) { return exact.id }
        let normalized = slug.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return document.outline.first { $0.anchor.trimmingCharacters(in: CharacterSet(charactersIn: "-")) == normalized }?.id
    }

    /// Only genuine parse errors surface in the reader. Style lints (tabs,
    /// trailing whitespace, heading-level jumps) are warnings and would be noise
    /// on ordinary documents — a viewer renders, it doesn't lint.
    private var errorDiagnostics: [DiagnosticIR] {
        document.diagnostics.filter { $0.severity == "error" }
    }

    // Tables and code blocks may exceed the reading measure; prose is capped.
    private func contentWidth(for block: BlockIR) -> CGFloat {
        switch block.kind {
        case "table", "code": .infinity
        default: CGFloat(settings.contentWidth)
        }
    }

    private func updateActiveHeading(from anchors: [HeadingAnchor]) {
        guard !anchors.isEmpty else { return }
        let threshold: CGFloat = 96
        let passed = anchors.filter { $0.minY <= threshold }
        let active = passed.max(by: { $0.minY < $1.minY })
            ?? anchors.min(by: { $0.minY < $1.minY })
        onActiveHeadingChange(active?.id)
    }

    // MARK: - Blocks

    private func blockView(_ block: BlockIR, topLevel: Bool = false) -> AnyView {
        switch block.kind {
        case "heading":
            let level = block.level ?? 1
            return AnyView(
                VStack(alignment: .leading, spacing: 7) {
                    Text(InlineRenderer.attributedString(
                        block.inlines,
                        base: settings.headingFont(level: level),
                        origin: document.origin,
                        highlight: searchQuery,
                        highlightIsActive: block.id == currentMatchID
                    ))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    if level <= 2 {
                        Divider()
                    }
                }
                .padding(.top, topLevel ? Theme.HeadingScale.topPadding(for: level) : 0)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(headingAnchor(for: block.id))
            )

        case "paragraph":
            return AnyView(paragraphView(block))

        case "blockquote":
            return AnyView(
                HStack(alignment: .top, spacing: 14) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.accentColor.opacity(0.55))
                        .frame(width: 3)
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(block.children) { child in blockView(child) }
                    }
                }
                .padding(.vertical, 2)
            )

        case "list":
            return AnyView(
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(block.items.enumerated()), id: \.element.id) { index, item in
                        HStack(alignment: .top, spacing: 10) {
                            listMarker(item: item, index: index, ordered: block.ordered ?? false, start: block.start)
                            VStack(alignment: .leading, spacing: 6) {
                                ForEach(item.children) { child in blockView(child) }
                            }
                        }
                    }
                }
            )

        case "code":
            return AnyView(CodeBlockView(language: block.language, code: block.code ?? "", settings: settings))

        case "table":
            return AnyView(DocumentTableView(headers: block.headers, rows: block.rows, origin: document.origin, settings: settings))

        case "thematic_break":
            return AnyView(Divider().padding(.vertical, 6))

        case "frontmatter":
            return AnyView(FrontmatterView(raw: block.raw ?? "", settings: settings))

        case "math", "html":
            return AnyView(CodeBlockView(language: block.kind, code: block.raw ?? "", settings: settings))

        default:
            if !block.children.isEmpty {
                return AnyView(
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(block.children) { child in blockView(child) }
                    }
                )
            }
            return AnyView(EmptyView())
        }
    }

    private func headingAnchor(for id: String) -> some View {
        GeometryReader { geo in
            Color.clear.preference(
                key: HeadingVisibilityKey.self,
                value: [HeadingAnchor(id: id, minY: geo.frame(in: .named(scrollSpace)).minY)]
            )
        }
    }

    @ViewBuilder
    private func listMarker(item: ListItemIR, index: Int, ordered: Bool, start: Int?) -> some View {
        if let checked = item.checked {
            Image(systemName: checked ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(checked ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.tertiary))
                .font(.body)
                .padding(.top, 1)
                .accessibilityLabel(checked ? "Completed task" : "Incomplete task")
        } else if ordered {
            Text("\((start ?? 1) + index).")
                .font(.body.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 24, alignment: .trailing)
        } else {
            Circle()
                .fill(.secondary)
                .frame(width: 5, height: 5)
                .padding(.top, 8)
                .frame(width: 16)
        }
    }

    /// Renders a paragraph, promoting any images to their own image views so a
    /// picture shows whether it stands alone, sits amid text, or is wrapped in a
    /// link (e.g. `[![badge](img)](url)`).
    @ViewBuilder
    private func paragraphView(_ block: BlockIR) -> some View {
        let images = collectImages(block.inlines)
        if images.isEmpty {
            paragraphText(block.inlines, blockID: block.id)
        } else {
            let textInlines = removingImages(block.inlines)
            let hasText = !textInlines.map(\.plainText).joined().trimmingCharacters(in: .whitespaces).isEmpty
            VStack(alignment: .leading, spacing: 10) {
                if hasText {
                    paragraphText(textInlines, blockID: block.id)
                }
                ForEach(Array(images.enumerated()), id: \.offset) { _, image in
                    MarkdownImageView(inline: image, origin: document.origin)
                }
            }
        }
    }

    private func paragraphText(_ inlines: [InlineIR], blockID: String) -> some View {
        Text(InlineRenderer.attributedString(
            inlines,
            base: settings.bodyFont,
            origin: document.origin,
            highlight: searchQuery,
            highlightIsActive: blockID == currentMatchID
        ))
        .lineSpacing(settings.lineSpacing)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Depth-first collection of image inlines, descending through links and
    /// emphasis so nested/linked images are found.
    private func collectImages(_ inlines: [InlineIR]) -> [InlineIR] {
        inlines.flatMap { inline -> [InlineIR] in
            inline.kind == "image" ? [inline] : collectImages(inline.children)
        }
    }

    /// A copy of the inline tree with image nodes removed, so surrounding text
    /// renders once (the images are shown separately as pictures).
    private func removingImages(_ inlines: [InlineIR]) -> [InlineIR] {
        inlines.compactMap { inline in
            if inline.kind == "image" { return nil }
            return InlineIR(
                id: inline.id,
                range: inline.range,
                kind: inline.kind,
                text: inline.text,
                children: removingImages(inline.children),
                url: inline.url,
                title: inline.title,
                alt: inline.alt
            )
        }
    }
}

// MARK: - Images

private struct MarkdownImageView: View {
    let inline: InlineIR
    let origin: String

    var body: some View {
        Group {
            if let url = InlineRenderer.resolvedURL(inline.url, origin: origin) {
                if url.isFileURL {
                    localImage(url)
                } else {
                    AsyncImage(url: url) { phase in
                        switch phase {
                        case .success(let image): image.resizable().scaledToFit()
                        case .failure: placeholder
                        case .empty: ProgressView().frame(maxWidth: .infinity, minHeight: 120)
                        @unknown default: placeholder
                        }
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadius))
        .overlay(RoundedRectangle(cornerRadius: Theme.cornerRadius).stroke(Color.readerHairline))
        .accessibilityLabel(inline.alt ?? "Image")
    }

    @ViewBuilder
    private func localImage(_ url: URL) -> some View {
        if let nsImage = NSImage(contentsOf: url) {
            Image(nsImage: nsImage).resizable().scaledToFit()
        } else {
            placeholder
        }
    }

    private var placeholder: some View {
        HStack(spacing: 8) {
            Image(systemName: "photo")
            Text(inline.alt?.isEmpty == false ? inline.alt! : "Image unavailable")
                .lineLimit(1)
        }
        .foregroundStyle(.secondary)
        .padding(.vertical, 20)
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity)
        .background(Color.readerSubtleFill)
    }
}

// MARK: - Frontmatter

/// Renders YAML/TOML frontmatter as a compact key/value card (inspired by Mud),
/// falling back to a raw block when it can't be parsed into simple pairs.
private struct FrontmatterView: View {
    let raw: String
    @ObservedObject var settings: ReaderSettings

    private var pairs: [(String, String)] {
        raw.split(separator: "\n").compactMap { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, let separator = trimmed.firstIndex(of: ":") else { return nil }
            let key = trimmed[..<separator].trimmingCharacters(in: .whitespaces)
            let value = trimmed[trimmed.index(after: separator)...]
                .trimmingCharacters(in: CharacterSet(charactersIn: " \"'"))
            guard !key.isEmpty else { return nil }
            return (key, value)
        }
    }

    var body: some View {
        if pairs.isEmpty {
            CodeBlockView(language: "frontmatter", code: raw, settings: settings)
        } else {
            VStack(alignment: .leading, spacing: 0) {
                Label("Frontmatter", systemImage: "text.alignleft")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                Divider()
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(pairs.enumerated()), id: \.offset) { _, pair in
                        HStack(alignment: .top, spacing: 12) {
                            Text(pair.0)
                                .font(.callout.weight(.medium))
                                .foregroundStyle(.secondary)
                                .frame(width: 120, alignment: .leading)
                            Text(pair.1)
                                .font(.callout)
                                .textSelection(.enabled)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                    }
                }
            }
            .background(Color.readerSubtleFill, in: RoundedRectangle(cornerRadius: Theme.cornerRadius))
            .overlay(RoundedRectangle(cornerRadius: Theme.cornerRadius).stroke(Color.readerHairline))
        }
    }
}

// MARK: - Diagnostics

private struct DiagnosticsView: View {
    let diagnostics: [DiagnosticIR]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Document diagnostics", systemImage: "exclamationmark.triangle.fill")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.orange)
            ForEach(diagnostics) { diagnostic in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(diagnostic.code)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                    Text(diagnostic.message)
                        .font(.caption)
                        .foregroundStyle(diagnostic.severity == "error" ? .red : .primary)
                    if let line = diagnostic.line {
                        Text("line \(line)").font(.caption2).foregroundStyle(.tertiary)
                    }
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: Theme.cornerRadius))
        .overlay(RoundedRectangle(cornerRadius: Theme.cornerRadius).stroke(.orange.opacity(0.25)))
    }
}
