import SwiftUI

struct EmptyDocumentView: View {
    let newFile: () -> Void
    let openFile: () -> Void
    let openWorkspace: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "doc.text.image")
                .font(.system(size: 46, weight: .regular))
                .foregroundStyle(.tertiary)
                .symbolRenderingMode(.hierarchical)

            VStack(spacing: 6) {
                Text("Nothing open yet")
                    .font(.title2.weight(.semibold))
                Text("Create a Markdown file, open an existing document, or browse a workspace.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 360)
            }

            HStack(spacing: 12) {
                Button(action: newFile) {
                    Label("New File…", systemImage: "doc.badge.plus")
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut("n", modifiers: .command)

                Button(action: openFile) {
                    Label("Open File…", systemImage: "doc")
                }
                .buttonStyle(.bordered)
                .keyboardShortcut("o", modifiers: .command)

                Button(action: openWorkspace) {
                    Label("Open Folder…", systemImage: "folder")
                }
                .buttonStyle(.bordered)
                .keyboardShortcut("o", modifiers: [.command, .shift])
            }
            .controlSize(.large)

            Text("Tip: use ⌘N to create a document, or drag a file onto the window.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}
