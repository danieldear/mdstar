import AppKit
import SwiftUI

/// Puts the window into full-size content mode with a unified toolbar.
///
/// This mirrors how Mud sets its document window up: the content view spans the
/// whole window and the toolbar sits over it, rather than the content starting
/// below the titlebar. The document's own top band, drawn by the stylesheet,
/// provides the transition beneath the bar.
struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async { [weak view] in
            guard let window = view?.window else { return }
            Self.configure(window)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let window = nsView.window else { return }
        Self.configure(window)
    }

    private static func configure(_ window: NSWindow) {
        if !window.styleMask.contains(.fullSizeContentView) {
            window.styleMask.insert(.fullSizeContentView)
        }
        window.toolbarStyle = .unified
        // Re-asserted every update: SwiftUI resets this as the toolbar rebuilds,
        // and a transparent titlebar leaves the bar without its material.
        window.titlebarAppearsTransparent = false
    }
}
