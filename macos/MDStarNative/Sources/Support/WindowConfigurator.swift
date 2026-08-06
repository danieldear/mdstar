import AppKit
import SwiftUI

/// Lets the SwiftUI detail surface extend through the unified titlebar.
///
/// Full-size content lets the system titlebar material blur the native reader
/// beneath it. No custom titlebar overlay is required.
struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async { [weak view] in
            if let window = view?.window { configure(window) }
        }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        if let window = view.window { configure(window) }
    }

    private func configure(_ window: NSWindow) {
        window.styleMask.insert(.fullSizeContentView)
        window.toolbarStyle = .unified
        // Keep AppKit's own titlebar material. With a native NSTextView below
        // it, this is the only layer that should create the fade/blur.
        window.titlebarAppearsTransparent = false
        window.titlebarSeparatorStyle = .none
    }
}
