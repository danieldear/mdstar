import AppKit
import SwiftUI

/// Keeps the native `List` used for the workspace sidebar pinned to its
/// leading edge. On recent macOS releases, an `NSOutlineView`-backed SwiftUI
/// list can retain a horizontal clip-view offset after a two-axis trackpad
/// gesture or a column resize. The result is especially confusing: the
/// sidebar surface stays put, but its search field and rows look cut off at
/// the leading edge.
///
/// This is intentionally a narrow AppKit bridge. It only configures the
/// enclosing scroll view for the sidebar `List`; document/source scroll views
/// retain their normal behaviour.
@MainActor
struct SidebarScrollPositionGuard: NSViewRepresentable {
    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSView {
        let marker = MarkerView(frame: .zero)
        marker.onLayout = { [weak coordinator = context.coordinator] marker in
            coordinator?.install(around: marker)
        }
        context.coordinator.install(around: marker)
        return marker
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        context.coordinator.install(around: nsView)
    }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        coordinator.removeObserver()
    }

    @MainActor
    final class Coordinator {
        private weak var scrollView: NSScrollView?
        private var boundsObserver: NSObjectProtocol?
        private var retryCount = 0

        func install(around marker: NSView) {
            guard let scrollView = sidebarScrollView(for: marker) else {
                // SwiftUI sometimes attaches the list's hosting hierarchy one
                // layout pass after its background view. Retry briefly rather
                // than leaving the sidebar unprotected in that race.
                guard retryCount < 3 else { return }
                retryCount += 1
                DispatchQueue.main.async { [weak self, weak marker] in
                    guard let self, let marker else { return }
                    self.install(around: marker)
                }
                return
            }

            retryCount = 0
            guard self.scrollView !== scrollView else {
                pinToLeadingEdge()
                return
            }

            removeObserver()
            self.scrollView = scrollView
            scrollView.hasHorizontalScroller = false
            scrollView.horizontalScrollElasticity = .none
            scrollView.contentView.postsBoundsChangedNotifications = true
            boundsObserver = NotificationCenter.default.addObserver(
                forName: NSView.boundsDidChangeNotification,
                object: scrollView.contentView,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in
                    self?.pinToLeadingEdge()
                }
            }
            pinToLeadingEdge()
        }

        func removeObserver() {
            if let boundsObserver {
                NotificationCenter.default.removeObserver(boundsObserver)
            }
            boundsObserver = nil
            scrollView = nil
        }

        /// SwiftUI places a `.background` sibling beside the List's internal
        /// scroll view, not inside it, on some macOS versions. In split mode
        /// the marker can briefly have a zero-sized frame while the sidebar
        /// scroll view is rebuilt, so geometric overlap is not reliable.
        /// Instead, select the narrow leftmost scroll view in the window: the
        /// only view matching that shape is the NavigationSplitView sidebar.
        private func sidebarScrollView(for marker: NSView) -> NSScrollView? {
            if let enclosing = enclosingScrollView(for: marker), isSourceList(enclosing) {
                return enclosing
            }

            guard let window = marker.window, let contentView = window.contentView else { return nil }
            let candidates = descendantScrollViews(in: contentView)
                .filter { scrollView in
                    let frame = scrollView.convert(scrollView.bounds, to: contentView)
                    return frame.minX <= 12 && frame.width <= 420 && frame.height > 80
                }

            // Prefer an AppKit table/outline list when SwiftUI exposes one;
            // fall back to the leftmost narrow scroll view for macOS releases
            // that host List through a private container.
            let sourceListCandidates = candidates.filter(isSourceList)
            let matchingCandidates = sourceListCandidates.isEmpty ? candidates : sourceListCandidates
            return matchingCandidates.min { lhs, rhs in
                let lhsFrame = lhs.convert(lhs.bounds, to: contentView)
                let rhsFrame = rhs.convert(rhs.bounds, to: contentView)
                return lhsFrame.minX < rhsFrame.minX
            }
        }

        private func enclosingScrollView(for view: NSView) -> NSScrollView? {
            var current: NSView? = view.superview
            while let candidate = current {
                if let scrollView = candidate as? NSScrollView {
                    return scrollView
                }
                current = candidate.superview
            }
            return nil
        }

        private func descendantScrollViews(in view: NSView) -> [NSScrollView] {
            let children = view.subviews.flatMap(descendantScrollViews(in:))
            return (view as? NSScrollView).map { [$0] + children } ?? children
        }

        private func isSourceList(_ scrollView: NSScrollView) -> Bool {
            guard let documentView = scrollView.documentView else { return false }
            return containsTableView(in: documentView)
        }

        private func containsTableView(in view: NSView) -> Bool {
            if view is NSTableView { return true }
            return view.subviews.contains(where: containsTableView(in:))
        }

        private func pinToLeadingEdge() {
            guard let scrollView, let documentView = scrollView.documentView else { return }
            let clipView = scrollView.contentView
            let leadingX = documentView.frame.minX
            guard abs(clipView.bounds.minX - leadingX) > 0.5 else { return }

            clipView.setBoundsOrigin(NSPoint(x: leadingX, y: clipView.bounds.minY))
            scrollView.reflectScrolledClipView(clipView)
        }
    }

    private final class MarkerView: NSView {
        var onLayout: ((NSView) -> Void)?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            onLayout?(self)
        }

        override func layout() {
            super.layout()
            onLayout?(self)
        }
    }
}
