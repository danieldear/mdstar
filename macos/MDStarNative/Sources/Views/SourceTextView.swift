import SwiftUI

/// Editable source editor used by split mode. The preview is rendered from the
/// same binding, so edits remain in-memory until the workspace saves them.
struct SourceTextView: View {
    @ObservedObject var workspace: WorkspaceStore

    private var source: Binding<String> {
        Binding(
            get: { workspace.rawText },
            set: { workspace.updateSource($0) }
        )
    }

    var body: some View {
        TextEditor(text: source)
            .font(.system(.callout, design: .monospaced))
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .background(.background)
            .accessibilityLabel("Document source editor")
    }
}
