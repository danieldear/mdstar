import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct WorkspaceWindow: View {
    @ObservedObject var workspace: WorkspaceStore
    @ObservedObject var navigation: NavigationStore
    @ObservedObject var inspector: InspectorStore

    @State private var columnVisibility: NavigationSplitViewVisibility = .all

    var body: some View {
        ZStack(alignment: .top) {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                WorkspaceSidebarView(workspace: workspace)
                    .navigationSplitViewColumnWidth(min: 232, ideal: 264, max: 340)
            } detail: {
                documentDetail
                    .workspaceInspector(isPresented: $inspector.isVisible, inspector: inspector, workspace: workspace)
                    .spaceTogglesSource(enabled: workspace.document != nil) { workspace.toggleSourcePreview() }
            }
            .navigationSplitViewStyle(.balanced)

            floatingToolbar
        }
        .ignoresSafeArea(.container, edges: .top)
        .onReceive(NotificationCenter.default.publisher(for: .mdstarOpenedDocument)) { notification in
            if let url = notification.object as? URL { navigation.record(url) }
        }
        .onDrop(of: [UTType.fileURL.identifier], isTargeted: nil, perform: acceptDrop)
        .task { workspace.restoreWorkspaceIfNeeded() }
    }

    // MARK: - Detail

    @ViewBuilder
    private var documentDetail: some View {
        Group {
            if workspace.isLoading {
                ProgressView().controlSize(.large)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if workspace.document != nil {
                documentModes
            } else if let error = workspace.errorMessage {
                errorState(error)
            } else {
                EmptyDocumentView(
                    openFile: workspace.chooseFile,
                    openWorkspace: workspace.chooseWorkspace
                )
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .textBackgroundColor))
        .overlay(alignment: .bottom) {
            if workspace.isFindPresented {
                FindBar(workspace: workspace)
                    .padding(.bottom, 22)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.easeOut(duration: 0.15), value: workspace.isFindPresented)
    }

    @ViewBuilder
    private var documentModes: some View {
        switch workspace.viewMode {
        case .view:
            renderer
        case .split:
            HSplitView {
                SourceTextView(text: workspace.rawText)
                    .frame(minWidth: 280)
                renderer
                    .frame(minWidth: 340)
            }
        }
    }

    @ViewBuilder
    private var renderer: some View {
        if let document = workspace.document {
            DocumentRenderer(
                document: document,
                focusedBlockID: workspace.focusedBlockID,
                searchQuery: workspace.isFindPresented ? workspace.findQuery : "",
                currentMatchID: workspace.currentMatchID,
                onOpenLink: openDocumentLink,
                onActiveHeadingChange: { workspace.setActiveHeading($0) }
            )
        }
    }

    private func errorState(_ error: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 34))
                .foregroundStyle(.orange)
            Text("Couldn’t open document").font(.title3.weight(.semibold))
            Text(error)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
            Button("Open File…", action: workspace.chooseFile)
                .buttonStyle(.borderedProminent)
        }
        .padding()
    }

    // MARK: - Floating toolbar

    private var floatingToolbar: some View {
        ZStack {
            HStack(spacing: 10) {
                // Gutter so the left cluster clears the window traffic lights.
                Color.clear.frame(width: 62, height: 1)

                GlassCluster {
                    ToolButton(icon: "sidebar.leading", help: "Toggle Sidebar", action: toggleSidebar)
                    ToolButton(icon: "chevron.backward", help: "Back", enabled: navigation.canGoBack, action: goBack)
                    ToolButton(icon: "chevron.forward", help: "Forward", enabled: navigation.canGoForward, action: goForward)
                }

                Spacer(minLength: 8)

                if workspace.document != nil {
                    GlassCluster {
                        Picker("View Mode", selection: $workspace.viewMode) {
                            ForEach(ReaderViewMode.allCases) { mode in
                                Text(mode.title).tag(mode)
                            }
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                        .fixedSize()
                        .padding(.horizontal, 1)
                    }
                }

                GlassCluster {
                    ToolButton(icon: "magnifyingglass", help: "Find in Document (⌘F)", enabled: workspace.document != nil, action: toggleFind)
                    ToolButton(icon: "arrow.clockwise", help: "Reload", enabled: workspace.selectedURL != nil, action: workspace.reload)
                    ToolButton(icon: "sidebar.trailing", help: "Toggle Inspector") { inspector.isVisible.toggle() }
                }

                GlassCluster {
                    Menu {
                        Button("Open File…", action: workspace.chooseFile)
                        Button("Open Folder…", action: workspace.chooseWorkspace)
                        Divider()
                        Button("Copy File Path") { copyCurrentPath() }.disabled(workspace.selectedURL == nil)
                        Button("Reveal in Finder") { revealCurrentFile() }.disabled(workspace.selectedURL == nil)
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.system(size: 15))
                            .frame(width: 30, height: 28)
                            .contentShape(Rectangle())
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                }
            }
            .padding(.horizontal, 14)

            if workspace.selectedURL != nil {
                BreadcrumbToolbarItem(workspaceURL: workspace.workspaceURL, fileURL: workspace.selectedURL)
                    .allowsHitTesting(false)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 13)
    }

    // MARK: - Actions

    private func toggleSidebar() {
        withAnimation(.easeInOut(duration: 0.2)) {
            columnVisibility = columnVisibility == .detailOnly ? .all : .detailOnly
        }
    }

    private func toggleFind() {
        if workspace.isFindPresented { workspace.dismissFind() } else { workspace.presentFind() }
    }

    private func goBack() {
        if let url = navigation.back() { workspace.openFile(url, recordHistory: false) }
    }

    private func goForward() {
        if let url = navigation.forward() { workspace.openFile(url, recordHistory: false) }
    }

    private func acceptDrop(_ providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }
        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
            guard let data = item as? Data,
                  let url = URL(dataRepresentation: data, relativeTo: nil) else { return }
            Task { @MainActor in self.workspace.openFile(url, recordHistory: true) }
        }
        return true
    }

    private func openDocumentLink(_ url: URL) {
        if url.isFileURL {
            workspace.openFile(url, recordHistory: true)
        } else {
            NSWorkspace.shared.open(url)
        }
    }

    private func copyCurrentPath() {
        guard let path = workspace.selectedURL?.path else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(path, forType: .string)
    }

    private func revealCurrentFile() {
        guard let url = workspace.selectedURL else { return }
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }
}

// MARK: - Floating glass toolbar components

/// A rounded Liquid-Glass capsule holding one or more floating controls.
private struct GlassCluster<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 2) { content }
            .padding(3)
            .floatingGlass(Capsule())
    }
}

/// A borderless icon button sized for the floating clusters, with its own hover
/// treatment (the capsule behind it stays glass).
private struct ToolButton: View {
    let icon: String
    let help: String
    var enabled: Bool = true
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 15))
                .frame(width: 30, height: 28)
                .background(
                    isHovering && enabled ? Color.primary.opacity(0.1) : .clear,
                    in: RoundedRectangle(cornerRadius: 7)
                )
                .contentShape(RoundedRectangle(cornerRadius: 7))
        }
        .buttonStyle(.plain)
        .foregroundStyle(enabled ? AnyShapeStyle(.primary) : AnyShapeStyle(.tertiary))
        .disabled(!enabled)
        .onHover { isHovering = $0 }
        .help(help)
    }
}

// MARK: - Inspector + focus helpers

private extension View {
    /// Space flips rendered ↔ split. `.onKeyPress` only fires while the reader
    /// (or a descendant) holds focus, so typing Space in the sidebar search is
    /// unaffected.
    @ViewBuilder
    func spaceTogglesSource(enabled: Bool, action: @escaping () -> Void) -> some View {
        if #available(macOS 14.0, *) {
            self
                .focusable(enabled)
                .focusEffectDisabled()
                .onKeyPress(.space) {
                    guard enabled else { return .ignored }
                    action()
                    return .handled
                }
        } else {
            self
        }
    }

    @ViewBuilder
    func workspaceInspector(isPresented: Binding<Bool>, inspector: InspectorStore, workspace: WorkspaceStore) -> some View {
        if #available(macOS 14.0, *) {
            self.inspector(isPresented: isPresented) {
                InspectorView(inspector: inspector, workspace: workspace)
                    .inspectorColumnWidth(min: 250, ideal: 280, max: 340)
            }
        } else {
            HStack(spacing: 0) {
                self
                if isPresented.wrappedValue {
                    Divider()
                    InspectorView(inspector: inspector, workspace: workspace)
                        .frame(minWidth: 250, idealWidth: 280, maxWidth: 340)
                }
            }
        }
    }
}
