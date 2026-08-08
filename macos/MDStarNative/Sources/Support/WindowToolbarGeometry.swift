import AppKit

/// Geometry shared by native and web-backed document surfaces that extend
/// beneath a transparent, full-size macOS toolbar.
enum WindowToolbarGeometry {
    @MainActor
    static func overlap(for view: NSView) -> CGFloat {
        guard let window = view.window else { return max(0, view.safeAreaInsets.top) }
        let frameInWindow = view.convert(view.bounds, to: nil)
        return max(
            view.safeAreaInsets.top,
            overlap(
                viewFrameInWindow: frameInWindow,
                contentLayoutRect: window.contentLayoutRect
            )
        )
    }

    static func overlap(
        viewFrameInWindow: NSRect,
        contentLayoutRect: NSRect
    ) -> CGFloat {
        let obscuredStart = max(viewFrameInWindow.minY, contentLayoutRect.maxY)
        return min(
            viewFrameInWindow.height,
            max(0, viewFrameInWindow.maxY - obscuredStart)
        )
    }
}
