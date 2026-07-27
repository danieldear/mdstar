import SwiftUI

/// Floating in-document find widget (inspired by Mud / Safari's find bar).
struct FindBar: View {
    @ObservedObject var workspace: WorkspaceStore
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.callout)

            TextField("Find in document", text: $workspace.findQuery)
                .textFieldStyle(.plain)
                .frame(minWidth: 150, maxWidth: 220)
                .focused($focused)
                .onSubmit { workspace.findNext() }
                .onChange(of: workspace.findQuery) { _ in workspace.recomputeFind() }

            Text(countLabel)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 52, alignment: .trailing)

            Divider().frame(height: 16)

            Button { workspace.findPrevious() } label: { Image(systemName: "chevron.up") }
                .disabled(workspace.findMatches.isEmpty)
                .help("Previous match (⇧⏎)")
            Button { workspace.findNext() } label: { Image(systemName: "chevron.down") }
                .disabled(workspace.findMatches.isEmpty)
                .help("Next match (⏎)")
            Button { workspace.dismissFind() } label: { Image(systemName: "xmark") }
                .help("Close (Esc)")
        }
        .buttonStyle(.borderless)
        .foregroundStyle(.primary)
        .padding(.horizontal, 11)
        .padding(.vertical, 7)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).stroke(Color.readerHairline))
        .shadow(color: .black.opacity(0.18), radius: 12, y: 4)
        .onAppear { focused = true }
        .onExitCommand { workspace.dismissFind() }
    }

    private var countLabel: String {
        if workspace.findQuery.trimmingCharacters(in: .whitespaces).isEmpty { return "" }
        guard !workspace.findMatches.isEmpty else { return "No results" }
        return "\(workspace.findIndex + 1) of \(workspace.findMatches.count)"
    }
}
