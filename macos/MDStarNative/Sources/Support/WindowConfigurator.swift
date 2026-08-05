import AppKit
import SwiftUI

/// Puts the window into full-size content mode.
///
/// A unified toolbar blurs whatever sits beneath it, so the document has to
/// extend under the titlebar for the effect to appear at all. Without this the
/// content stops at the toolbar's lower edge and the bar renders flat, with
/// nothing behind it to sample.
struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        // The view is not in a window during make; configure once it is.
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
        guard !window.styleMask.contains(.fullSizeContentView) else { return }
        window.styleMask.insert(.fullSizeContentView)
        // The titlebar keeps its material; only the content extends beneath it.
        window.titlebarAppearsTransparent = false
        window.isMovableByWindowBackground = false
    }
}
