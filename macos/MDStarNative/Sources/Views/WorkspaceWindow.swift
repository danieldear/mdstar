import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Native macOS workspace shell. Window chrome is supplied by SwiftUI's
/// toolbar system; the document surface does not draw titlebar overlays.
struct WorkspaceWindow: View {
    @ObservedObject var workspace: WorkspaceStore
    @ObservedObject var navigation: NavigationStore
    @ObservedObject var inspector: InspectorStore
    @ObservedObject var settings: ReaderSettings
    @ObservedObject var annotations: AnnotationStore

    @State private var selection: SelectedText?
    @State private var webSelection: WebSelection?
    @State private var pendingComment: SelectedText?
    @State private var commentDraft = ""
    @State private var columnVisibility: NavigationSplitViewVisibility = .all

    var body: some View {
        NavigationSplitView(columnVisibility: $columnVisibility) {
            WorkspaceSidebarView(workspace: workspace)
                .navigationSplitViewColumnWidth(min: 232, ideal: 264, max: 340)
        } detail: {
            detailContent
                .workspaceInspector(
                    isPresented: $inspector.isVisible,
                    inspector: inspector,
                    workspace: workspace,
                    annotations: annotations
                )
                // The native reader has its own safe content inset and a
                // titlebar material layer, so document text can scroll behind
                // the toolbar and fade naturally rather than being clipped.
                .ignoresSafeArea(.container, edges: .top)
        }
        .navigationSplitViewStyle(.balanced)
        .toolbar { toolbarContent }
        .onReceive(NotificationCenter.default.publisher(for: .mdstarOpenedDocument)) { notification in
            if let url = notification.object as? URL {
                navigation.record(url)
            }
        }
        .onDrop(of: [UTType.fileURL.identifier], isTargeted: nil, perform: acceptDrop)
        .onReceive(NotificationCenter.default.publisher(for: .mdstarAddHighlight)) { _ in addHighlight() }
        .onReceive(NotificationCenter.default.publisher(for: .mdstarAddComment)) { _ in beginComment() }
        .sheet(item: $pendingComment) { target in
            CommentComposer(
                snippet: target.text,
                note: $commentDraft,
                onCancel: { pendingComment = nil; commentDraft = "" },
                onSave: { saveComment(for: target) }
            )
        }
        .background(WindowConfigurator())
        .task { workspace.restoreWorkspaceIfNeeded() }
    }

    // MARK: - Native Toolbar

    /// The standard sidebar toggle is provided by NavigationSplitView. These
    /// history controls use the navigation placement so they sit beside it.
    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        // Leading: history controls sit next to the system sidebar toggle.
        ToolbarItemGroup(placement: .navigation) {
            Button(action: goBack) {
                Label("Back", systemImage: "chevron.backward")
            }
            .disabled(!navigation.canGoBack)
            .help("Back")

            Button(action: goForward) {
                Label("Forward", systemImage: "chevron.forward")
            }
            .disabled(!navigation.canGoForward)
            .help("Forward")
        }

        // Center: the breadcrumb always names the active document; a compact
        // switcher appears beside it once more than one file is open.
        ToolbarItem(placement: .principal) {
            HStack(spacing: 6) {
                BreadcrumbToolbarItem(
                    workspaceURL: workspace.workspaceURL,
                    fileURL: workspace.selectedURL
                )
                if workspace.openDocumentURLs.count > 1 {
                    DocumentPickerToolbarItem(
                        urls: workspace.openDocumentURLs,
                        selectedURL: workspace.selectedTabURL,
                        select: workspace.selectTab,
                        close: workspace.closeTab
                    )
                }
            }
        }

        // Trailing: document operations and the inspector.
        ToolbarItemGroup(placement: .primaryAction) {
            Button(action: workspace.chooseFile) {
                Label("Open File", systemImage: "folder")
            }
            .help("Open File")

            if workspace.isFindPresented {
                DocumentSearchField(workspace: workspace)
            } else {
                Button(action: toggleFind) {
                    Label("Find", systemImage: "magnifyingglass")
                }
                .disabled(workspace.document == nil)
                .help("Find in Document (⌘F)")
            }

            Button(action: workspace.reload) {
                Label("Reload", systemImage: "arrow.clockwise")
            }
            .disabled(workspace.selectedURL == nil)
            .help("Reload")

            Button {
                _ = workspace.save()
            } label: {
                Label("Save", systemImage: "square.and.arrow.down")
            }
            .disabled(!workspace.isDirty)
            .help("Save (⌘S)")

            Menu {
                Picker("Reading Mode", selection: $workspace.viewMode) {
                    ForEach(ReaderViewMode.allCases) { mode in
                        Label(mode.title, systemImage: mode.systemImage).tag(mode)
                    }
                }
                .pickerStyle(.inline)
                .disabled(workspace.document == nil)

                Divider()

                Button("Toggle Source") {
                    workspace.toggleSourcePreview()
                }
                .disabled(workspace.document == nil)
            } label: {
                Label("Reading Mode", systemImage: workspace.viewMode.systemImage)
            }
            .disabled(workspace.document == nil)
            .help("Reading Mode")

            Button {
                inspector.isVisible.toggle()
            } label: {
                Label(
                    inspector.isVisible ? "Hide Inspector" : "Show Inspector",
                    systemImage: "sidebar.trailing"
                )
            }
            .help(inspector.isVisible ? "Hide Inspector" : "Show Inspector")
        }
    }

    // MARK: - Detail

    /// The tab strip is native toolbar chrome rather than document content.
    private var detailContent: some View {
        documentDetail
    }

    @ViewBuilder
    private var documentDetail: some View {
        Group {
            if workspace.isLoading {
                ProgressView()
                    .controlSize(.large)
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
        .onReceive(NotificationCenter.default.publisher(for: .mdstarAddHighlight)) { _ in
            addHighlight()
        }
        .onReceive(NotificationCenter.default.publisher(for: .mdstarAddComment)) { _ in
            beginComment()
        }
        .sheet(item: $pendingComment) { target in
            CommentComposer(
                snippet: target.text,
                note: $commentDraft,
                onCancel: {
                    pendingComment = nil
                    commentDraft = ""
                },
                onSave: {
                    saveComment(for: target)
                }
            )
        }
    }

    // MARK: - Annotations

    var canAnnotate: Bool { activeSelection != nil && workspace.document != nil }

    /// Normalises the two engines' selections into one shape. The web reader
    /// reports offsets within a block; TextKit reports an absolute range.
    private var activeSelection: SelectedText? {
        if settings.engine == .web {
            guard let webSelection else { return nil }
            return SelectedText(
                range: NSRange(
                    location: webSelection.start,
                    length: max(0, webSelection.end - webSelection.start)
                ),
                text: webSelection.text,
                blockID: webSelection.blockID
            )
        }
        return selection
    }

    func addHighlight() {
        guard let target = activeSelection, let document = workspace.document else { return }
        annotations.add(
            kind: .highlight,
            range: target.range,
            snippet: target.text,
            blockID: target.blockID,
            documentID: document.documentID
        )
    }

    func beginComment() {
        guard let target = activeSelection, workspace.document != nil else { return }
        commentDraft = ""
        pendingComment = target
    }

    private func saveComment(for target: SelectedText) {
        if let document = workspace.document {
            annotations.add(
                kind: .comment,
                range: target.range,
                snippet: target.text,
                note: commentDraft,
                blockID: target.blockID,
                documentID: document.documentID
            )
        }
        pendingComment = nil
        commentDraft = ""
    }

    @ViewBuilder
    private var documentModes: some View {
        switch workspace.viewMode {
        case .view:
            renderer
        case .split:
            HSplitView {
                SourceTextView(workspace: workspace)
                .frame(minWidth: 300, idealWidth: 460)
                renderer
                    .frame(minWidth: 360)
            }
        }
    }

    @ViewBuilder
    private var renderer: some View {
        if let document = workspace.document {
            switch settings.engine {
            case .web:
                WebReaderView(
                    document: document,
                    fileURL: workspace.selectedURL,
                    settings: settings,
                    annotations: annotations,
                    source: workspace.rawText,
                    focusedBlockID: workspace.focusedBlockID,
                    searchQuery: workspace.isFindPresented ? workspace.findQuery : "",
                    onOpenLink: openDocumentLink,
                    onActiveHeadingChange: { workspace.setActiveHeading($0) },
                    onSelectionChange: { webSelection = $0 },
                    onFindResults: { count, index in
                        workspace.reportFindResults(count: count, index: index)
                    }
                )
                .ignoresSafeArea(.container, edges: .top)
            case .textKit:
                TextKitReaderView(
                    document: document,
                    settings: settings,
                    annotations: annotations,
                    focusedBlockID: workspace.focusedBlockID,
                    searchQuery: workspace.isFindPresented ? workspace.findQuery : "",
                    currentMatchID: workspace.currentMatchID,
                    onOpenLink: openDocumentLink,
                    onActiveHeadingChange: { workspace.setActiveHeading($0) },
                    onSelectionChange: { selection = $0 }
                )
            }
        }
    }

    // MARK: - Selection annotations

    private func addHighlight() {
        guard let selection, let document = workspace.document,
              selection.documentID == document.documentID else { return }
        annotations.add(
            kind: .highlight,
            range: selection.range,
            snippet: selection.text,
            blockID: selection.blockID,
            segments: selection.segments,
            documentID: document.documentID
        )
        inspector.selectedSection = .highlights
        inspector.isVisible = true
    }

    private func beginComment() {
        guard let selection, let document = workspace.document,
              selection.documentID == document.documentID else { return }
        commentDraft = ""
        pendingComment = selection
    }

    private func saveComment(for selection: SelectedText) {
        guard let document = workspace.document,
              selection.documentID == document.documentID else { return }
        annotations.add(
            kind: .comment,
            range: selection.range,
            snippet: selection.text,
            note: commentDraft,
            blockID: selection.blockID,
            segments: selection.segments,
            documentID: document.documentID
        )
        pendingComment = nil
        commentDraft = ""
        inspector.selectedSection = .comments
        inspector.isVisible = true
    }

    private func errorState(_ error: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 34))
                .foregroundStyle(.orange)
            Text("Couldn’t open document")
                .font(.title3.weight(.semibold))
            Text(error)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
            Button("Open File…", action: workspace.chooseFile)
                .buttonStyle(.borderedProminent)
        }
        .padding()
    }

    // MARK: - Actions

    private func toggleFind() {
        if workspace.isFindPresented {
            workspace.dismissFind()
        } else {
            workspace.presentFind()
        }
    }

    private func goBack() {
        if let url = navigation.back() {
            workspace.openFile(url, recordHistory: false)
        }
    }

    private func goForward() {
        if let url = navigation.forward() {
            workspace.openFile(url, recordHistory: false)
        }
    }

    private func acceptDrop(_ providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }

        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
            guard let data = item as? Data,
                  let url = URL(dataRepresentation: data, relativeTo: nil) else {
                return
            }

            Task { @MainActor in
                workspace.openFile(url, recordHistory: true)
            }
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
}

// MARK: - Inspector

private extension View {
    @ViewBuilder
    func workspaceInspector(
        isPresented: Binding<Bool>,
        inspector: InspectorStore,
        workspace: WorkspaceStore,
        annotations: AnnotationStore
    ) -> some View {
        if #available(macOS 14.0, *) {
            self.inspector(isPresented: isPresented) {
                InspectorView(inspector: inspector, workspace: workspace, annotations: annotations)
                    .inspectorColumnWidth(min: 250, ideal: 280, max: 340)
            }
        } else {
            HStack(spacing: 0) {
                self
                if isPresented.wrappedValue {
                    Divider()
                    InspectorView(inspector: inspector, workspace: workspace, annotations: annotations)
                        .frame(minWidth: 250, idealWidth: 280, maxWidth: 340)
                }
            }
        }
    }
}
