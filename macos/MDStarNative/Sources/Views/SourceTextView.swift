import AppKit
import SwiftUI

/// Editable source editor used by split mode.
///
/// This is deliberately an AppKit text view rather than SwiftUI `TextEditor`.
/// `TextEditor` retains its private horizontal scroll position as an `HSplitView`
/// is reparented or resized; that is why the left edge of source lines could
/// disappear after opening the inspector.  Owning the scroll view lets this
/// editor preserve normal code-style horizontal scrolling while resetting only
/// after a real container-width change.
struct SourceTextView: NSViewRepresentable {
    @ObservedObject var workspace: WorkspaceStore

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = SourceEditorScrollView(source: workspace.rawText)
        let textView = scrollView.editorTextView
        textView.delegate = context.coordinator
        textView.handleCommandKey = { [weak coordinator = context.coordinator] event in
            coordinator?.handleCommandKey(event) ?? false
        }

        context.coordinator.scrollView = scrollView
        context.coordinator.textView = textView
        context.coordinator.documentURL = workspace.selectedURL?.standardizedFileURL
        context.coordinator.lastWidth = scrollView.contentSize.width
        context.coordinator.requestScrollToDocumentStart()
        return scrollView
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: Coordinator) {
        coordinator.tearDown()
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.workspace = workspace
        let selectedURL = workspace.selectedURL?.standardizedFileURL
        let documentChanged = context.coordinator.documentURL != selectedURL
        if documentChanged {
            context.coordinator.documentURL = selectedURL
        }

        if let textView = context.coordinator.textView, textView.string != workspace.rawText {
            context.coordinator.isApplyingModel = true
            textView.string = workspace.rawText
            context.coordinator.isApplyingModel = false
        }

        if documentChanged {
            context.coordinator.requestScrollToDocumentStart()
        }

        let width = scrollView.contentSize.width
        if abs(width - context.coordinator.lastWidth) > 0.5 {
            context.coordinator.lastWidth = width
            // A width change means the split or inspector changed.  Always
            // reveal the source's leading edge; do not reset vertical reading
            // position or user-initiated horizontal scrolling otherwise.
            let current = scrollView.contentView.bounds.origin
            scrollView.contentView.scroll(to: NSPoint(x: 0, y: current.y))
            scrollView.reflectScrolledClipView(scrollView.contentView)
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(workspace: workspace) }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate, NSPopoverDelegate {
        var workspace: WorkspaceStore
        weak var scrollView: SourceEditorScrollView?
        weak var textView: SlashCommandTextView?
        var documentURL: URL?
        var lastWidth: CGFloat = 0
        var isApplyingModel = false
        private var scrollResetGeneration = 0
        private var commandKeyMonitor: Any?
        private let paletteModel = SlashCommandPaletteModel()
        private var commandContext: SlashCommandContext?
        private lazy var commandPopover: NSPopover = {
            let popover = NSPopover()
            popover.behavior = .transient
            popover.animates = true
            popover.delegate = self
            popover.contentSize = NSSize(width: 310, height: 390)
            popover.contentViewController = NSHostingController(
                rootView: SlashCommandPalette(model: paletteModel) { [weak self] command in
                    self?.apply(command)
                }
            )
            return popover
        }()

        init(workspace: WorkspaceStore) {
            self.workspace = workspace
        }

        /// AppKit does not know its final clip-view geometry until after the
        /// representable has entered the window. Defer the reset by one runloop
        /// so opening Split mode or switching files always exposes byte zero.
        func requestScrollToDocumentStart() {
            scrollResetGeneration += 1
            let generation = scrollResetGeneration
            DispatchQueue.main.async { [weak self] in
                guard let self, self.scrollResetGeneration == generation else { return }
                self.scrollView?.scrollToDocumentStart()
            }
        }

        func textDidChange(_ notification: Notification) {
            guard !isApplyingModel, let textView else { return }
            workspace.updateSource(textView.string)
            reportEditorLocation()
            refreshCommandPalette()
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            guard !isApplyingModel else { return }
            reportEditorLocation()
            refreshCommandPalette()
        }

        private func reportEditorLocation() {
            guard let textView else { return }
            workspace.setEditorCaret(utf16Offset: textView.selectedRange().location)
        }

        func handleCommandKey(_ event: NSEvent) -> Bool {
            guard commandPopover.isShown else { return false }

            switch event.keyCode {
            case 125: // Down arrow
                paletteModel.moveSelection(by: 1)
                return true
            case 126: // Up arrow
                paletteModel.moveSelection(by: -1)
                return true
            case 36, 48, 76: // Return, Tab, keypad Enter
                guard let command = paletteModel.selectedCommand else { return true }
                apply(command)
                return true
            case 53: // Escape
                dismissCommandPalette()
                return true
            default:
                return false
            }
        }

        private func refreshCommandPalette() {
            guard let textView,
                  let context = SlashCommandEngine.context(
                    in: textView.string,
                    selectedRange: textView.selectedRange()
                  ) else {
                dismissCommandPalette()
                return
            }

            commandContext = context
            paletteModel.update(commands: SlashCommandEngine.filteredCommands(matching: context.query))
            showCommandPaletteIfNeeded(relativeTo: textView)
        }

        private func showCommandPaletteIfNeeded(relativeTo textView: NSTextView) {
            guard !commandPopover.isShown else { return }

            let selectedRange = textView.selectedRange()
            let screenRect = textView.firstRect(
                forCharacterRange: NSRange(location: selectedRange.location, length: 0),
                actualRange: nil
            )
            let windowRect = textView.window?.convertFromScreen(screenRect) ?? .zero
            let localRect = textView.convert(windowRect, from: nil)
            let anchor = NSRect(
                x: localRect.minX,
                y: localRect.minY,
                width: max(1, localRect.width),
                height: max(1, localRect.height)
            )
            commandPopover.show(relativeTo: anchor, of: textView, preferredEdge: .maxY)
            startCommandKeyMonitor()

            // A popover has its own window and can temporarily disturb the
            // responder chain. Keep ordinary typing in the source editor while
            // the local monitor below owns only palette navigation keys.
            DispatchQueue.main.async { [weak textView] in
                textView?.window?.makeFirstResponder(textView)
            }
        }

        private func dismissCommandPalette() {
            commandContext = nil
            if commandPopover.isShown { commandPopover.performClose(nil) }
            stopCommandKeyMonitor()
        }

        private func startCommandKeyMonitor() {
            guard commandKeyMonitor == nil else { return }
            commandKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self, self.commandPopover.isShown else { return event }
                return self.handleCommandKey(event) ? nil : event
            }
        }

        private func stopCommandKeyMonitor() {
            guard let commandKeyMonitor else { return }
            NSEvent.removeMonitor(commandKeyMonitor)
            self.commandKeyMonitor = nil
        }

        func popoverDidClose(_ notification: Notification) {
            commandContext = nil
            stopCommandKeyMonitor()
        }

        func tearDown() {
            if commandPopover.isShown { commandPopover.close() }
            stopCommandKeyMonitor()
        }

        private func apply(_ command: SlashCommand) {
            guard let textView, let context = commandContext else { return }
            guard textView.shouldChangeText(in: context.replacementRange, replacementString: command.insertion) else {
                return
            }

            textView.textStorage?.replaceCharacters(in: context.replacementRange, with: command.insertion)
            textView.didChangeText()
            textView.setSelectedRange(
                NSRange(
                    location: context.replacementRange.location + command.selection.location,
                    length: command.selection.length
                )
            )
            textView.window?.makeFirstResponder(textView)
            textView.scrollRangeToVisible(textView.selectedRange())
            dismissCommandPalette()
        }
    }
}

/// An AppKit source editor that deliberately extends beneath the transparent
/// unified toolbar while keeping the start of the document unobscured.
///
/// `automaticallyAdjustsContentInsets` cannot solve this window configuration:
/// AppKit only performs that adjustment for a non-transparent titlebar. MD Star
/// uses a full-size content view with a transparent titlebar so the renderer can
/// retain its frosted fade. We therefore mirror this view's safe-area overlap
/// into the scroll view's content/scroller insets ourselves.
@MainActor
final class SourceEditorScrollView: NSScrollView {
    let editorTextView: SlashCommandTextView
    private(set) var toolbarContentInset: CGFloat = 0
    private var toolbarInsetUpdateScheduled = false

    init(source: String) {
        editorTextView = SlashCommandTextView(frame: .zero)
        super.init(frame: NSRect(x: 0, y: 0, width: 640, height: 480))

        drawsBackground = true
        backgroundColor = .windowBackgroundColor
        hasVerticalScroller = true
        hasHorizontalScroller = true
        autohidesScrollers = true
        borderType = .noBorder
        automaticallyAdjustsContentInsets = false
        contentInsets = NSEdgeInsets()
        scrollerInsets = NSEdgeInsets()
        setContentHuggingPriority(.defaultLow, for: .horizontal)
        setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        let viewportSize = contentSize
        editorTextView.frame = NSRect(origin: .zero, size: viewportSize)
        editorTextView.minSize = viewportSize
        editorTextView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        editorTextView.isEditable = true
        editorTextView.isSelectable = true
        editorTextView.isRichText = false
        editorTextView.allowsUndo = true
        editorTextView.usesFindBar = true
        // Markdown source is code-like text. Respect every character the user
        // types instead of inheriting global macOS substitutions such as
        // `---` -> em dash or straight quotes -> smart quotes.
        editorTextView.isAutomaticDashSubstitutionEnabled = false
        editorTextView.isAutomaticQuoteSubstitutionEnabled = false
        editorTextView.isAutomaticTextReplacementEnabled = false
        editorTextView.isAutomaticSpellingCorrectionEnabled = false
        editorTextView.font = .monospacedSystemFont(
            ofSize: NSFont.systemFontSize,
            weight: .regular
        )
        editorTextView.textColor = .labelColor
        editorTextView.drawsBackground = false
        editorTextView.insertionPointColor = .controlAccentColor
        editorTextView.textContainerInset = NSSize(width: 16, height: 14)
        editorTextView.isHorizontallyResizable = true
        editorTextView.isVerticallyResizable = true
        editorTextView.textContainer?.widthTracksTextView = false
        editorTextView.textContainer?.containerSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        editorTextView.string = source
        documentView = editorTextView
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unavailable")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updateToolbarContentInset()
        scheduleToolbarContentInsetUpdate()
    }

    override func layout() {
        super.layout()
        scheduleToolbarContentInsetUpdate()
    }

    /// Refresh the protected titlebar/toolbar region after AppKit has attached
    /// the representable to its real window hierarchy.
    func updateToolbarContentInset() {
        // A SwiftUI ancestor using `ignoresSafeArea` can report zero through
        // NSView.safeAreaInsets even though this view still occupies the
        // titlebar region. NSWindow.contentLayoutRect remains authoritative.
        applyToolbarContentInset(WindowToolbarGeometry.overlap(for: self))
    }

    private func scheduleToolbarContentInsetUpdate() {
        guard window != nil, !toolbarInsetUpdateScheduled else { return }
        toolbarInsetUpdateScheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.toolbarInsetUpdateScheduled = false
            self.updateToolbarContentInset()
        }
    }

    /// Kept internal so the geometry can be regression-tested without showing
    /// a live window. Production callers should use `updateToolbarContentInset`.
    func applyToolbarContentInset(_ top: CGFloat) {
        let inset = max(0, top)
        guard abs(toolbarContentInset - inset) > 0.5 else { return }

        let wasAtDocumentStart = abs(contentView.bounds.origin.y - documentStartY) <= 0.5
        toolbarContentInset = inset
        contentInsets = NSEdgeInsets(top: inset, left: 0, bottom: 0, right: 0)
        scrollerInsets = NSEdgeInsets(top: inset, left: 0, bottom: 0, right: 0)

        if wasAtDocumentStart {
            scrollToDocumentStart()
        }
    }

    private var documentStartY: CGFloat {
        -contentInsets.top
    }

    func scrollToDocumentStart() {
        layoutSubtreeIfNeeded()
        editorTextView.layoutManager?.ensureLayout(for: editorTextView.textContainer!)

        var topBounds = contentView.bounds
        topBounds.origin = NSPoint(x: 0, y: documentStartY)
        let constrained = contentView.constrainBoundsRect(topBounds)
        contentView.scroll(to: constrained.origin)
        reflectScrolledClipView(contentView)
    }
}

/// Intercepts only the navigation keys used while the slash palette is open.
/// All ordinary text-system behavior remains owned by `NSTextView`.
final class SlashCommandTextView: NSTextView {
    var handleCommandKey: ((NSEvent) -> Bool)?

    override func keyDown(with event: NSEvent) {
        if handleCommandKey?(event) == true { return }
        super.keyDown(with: event)
    }
}
