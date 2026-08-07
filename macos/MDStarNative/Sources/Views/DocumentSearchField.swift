import AppKit
import SwiftUI

/// Toolbar find control.
///
/// Wraps `NSSearchField` rather than a SwiftUI `TextField` because AppKit
/// already provides the behavior asked for here: a recents menu persisted
/// across launches, the clear button, and the standard focus ring. Match
/// navigation sits beside it so ⏎ / ⇧⏎ step through hits without leaving the
/// field.
struct DocumentSearchField: View {
    @ObservedObject var workspace: WorkspaceStore

    /// Recomputing on every edit is what keeps the match count in step with the
    /// highlighting. Binding straight to `findQuery` updates the highlight but
    /// leaves `findMatches` stale, which reported "No results" over visibly
    /// highlighted text.
    private var queryBinding: Binding<String> {
        Binding(
            get: { workspace.findQuery },
            set: { newValue in
                workspace.findQuery = newValue
                workspace.recomputeFind()
            }
        )
    }

    var body: some View {
        HStack(spacing: 6) {
            SearchFieldRepresentable(
                text: queryBinding,
                placeholder: "Find in Document",
                onSubmit: { workspace.findNext() },
                onSubmitBackwards: { workspace.findPrevious() },
                onCancel: { workspace.dismissFind() },
                onCommitRecent: { workspace.rememberSearch($0) },
                onEndEditing: {
                    // Collapse back to the toolbar icon once the field is done
                    // and empty, instead of occupying the toolbar permanently.
                    if !workspace.hasActiveSearch { workspace.dismissFind() }
                }
            )
            .frame(width: 190)

            if workspace.hasActiveSearch {
                Text(workspace.matchSummary)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(minWidth: 58, alignment: .trailing)
                    .accessibilityLabel(workspace.matchSummary)

                Button { workspace.findPrevious() } label: {
                    Image(systemName: "chevron.up")
                }
                .disabled(workspace.findMatches.isEmpty)
                .help("Previous Match (⇧⏎)")

                Button { workspace.findNext() } label: {
                    Image(systemName: "chevron.down")
                }
                .disabled(workspace.findMatches.isEmpty)
                .help("Next Match (⏎)")
            }
        }
    }
}

private struct SearchFieldRepresentable: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let onSubmit: () -> Void
    let onSubmitBackwards: () -> Void
    let onCancel: () -> Void
    let onCommitRecent: (String) -> Void
    let onEndEditing: () -> Void

    func makeNSView(context: Context) -> NSSearchField {
        let field = SubmitAwareSearchField()
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.sendsWholeSearchString = false
        field.sendsSearchStringImmediately = true
        field.onShiftReturn = onSubmitBackwards

        // Persisted recents menu, capped so the list stays scannable.
        field.recentsAutosaveName = "mdstar.find.recents"
        field.maximumRecents = 6
        field.searchMenuTemplate = Self.recentsMenu()

        // Take focus as the field appears so typing and Escape land here
        // without an extra click.
        DispatchQueue.main.async { [weak field] in
            guard let field else { return }
            field.window?.makeFirstResponder(field)
        }

        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text {
            field.stringValue = text
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    /// Template driving the recents popup. AppKit fills the tagged items in.
    private static func recentsMenu() -> NSMenu {
        let menu = NSMenu(title: "Recent Searches")

        let header = NSMenuItem(title: "Recent Searches", action: nil, keyEquivalent: "")
        header.tag = Int(NSSearchField.recentsTitleMenuItemTag)
        menu.addItem(header)

        let recents = NSMenuItem(title: "Item", action: nil, keyEquivalent: "")
        recents.tag = Int(NSSearchField.recentsMenuItemTag)
        menu.addItem(recents)

        let empty = NSMenuItem(title: "No Recent Searches", action: nil, keyEquivalent: "")
        empty.tag = Int(NSSearchField.noRecentsMenuItemTag)
        menu.addItem(empty)

        menu.addItem(.separator())

        let clear = NSMenuItem(title: "Clear Recent Searches", action: nil, keyEquivalent: "")
        clear.tag = Int(NSSearchField.clearRecentsMenuItemTag)
        menu.addItem(clear)

        return menu
    }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var parent: SearchFieldRepresentable

        init(_ parent: SearchFieldRepresentable) {
            self.parent = parent
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            parent.onEndEditing()
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSSearchField else { return }
            parent.text = field.stringValue
        }

        func control(
            _ control: NSControl,
            textView: NSTextView,
            doCommandBy selector: Selector
        ) -> Bool {
            switch selector {
            case #selector(NSResponder.insertNewline(_:)):
                // Committing the term also records it in the recents menu.
                let term = control.stringValue.trimmingCharacters(in: .whitespaces)
                if !term.isEmpty { parent.onCommitRecent(term) }
                parent.onSubmit()
                return true
            case #selector(NSResponder.cancelOperation(_:)):
                parent.onCancel()
                return true
            default:
                return false
            }
        }
    }
}

/// `NSSearchField` reports ⇧⏎ through `keyDown` rather than the delegate's
/// command selectors, so backwards search is handled here.
private final class SubmitAwareSearchField: NSSearchField {
    var onShiftReturn: (() -> Void)?

    override func keyDown(with event: NSEvent) {
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        if isReturn, event.modifierFlags.contains(.shift) {
            onShiftReturn?()
            return
        }
        super.keyDown(with: event)
    }
}
