import AppKit
import SwiftUI

/// Puts the window into full-size content mode with a unified toolbar.
///
/// This mirrors how Mud sets its document window up: the content view spans the
/// whole window and the toolbar sits over it, rather than the content starting
/// below the titlebar. The document's own top band, drawn by the stylesheet,
/// provides the transition beneath the bar.
struct WindowConfigurator: NSViewRepresentable {
    /// Incremented when the detail hierarchy switches between one pane and an
    /// `HSplitView`. It gives AppKit one final layout pass after SwiftUI has
    /// installed or removed the split view, matching the reflow a manual
    /// window resize previously triggered.
    let layoutRevision: Int

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async { [weak view] in
            guard let window = view?.window else { return }
            Self.configure(window)
            context.coordinator.refreshLayoutIfNeeded(in: window, revision: layoutRevision)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let window = nsView.window else { return }
        Self.configure(window)
        context.coordinator.refreshLayoutIfNeeded(in: window, revision: layoutRevision)
    }

    private static func configure(_ window: NSWindow) {
        if !window.styleMask.contains(.fullSizeContentView) {
            window.styleMask.insert(.fullSizeContentView)
        }
        window.toolbarStyle = .unified
        // The titlebar draws nothing of its own: its material cannot sample the
        // web view, so the page supplies the blur instead and the bar must let
        // it show through. Re-asserted every update because SwiftUI resets this
        // as the toolbar rebuilds.
        window.titlebarAppearsTransparent = true
    }

    @MainActor
    final class Coordinator {
        private var lastLayoutRevision: Int?

        func refreshLayoutIfNeeded(in window: NSWindow, revision: Int) {
            guard lastLayoutRevision != revision else { return }
            lastLayoutRevision = revision

            // A NavigationSplitView owns AppKit split views below SwiftUI. The
            // first layout pass can happen before its new detail HSplitView is
            // attached, leaving the sidebar's source list with stale geometry.
            // Defer once so the final hierarchy is present, then explicitly
            // lay it out rather than manipulating the List's private scroll
            // view or retaining an accidental horizontal offset.
            DispatchQueue.main.async { [weak window] in
                guard let window, let contentView = window.contentView else { return }
                contentView.needsLayout = true
                contentView.layoutSubtreeIfNeeded()
                window.invalidateShadow()
            }
        }
    }
}
