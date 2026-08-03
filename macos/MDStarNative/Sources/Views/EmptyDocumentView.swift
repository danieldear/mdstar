import SwiftUI

struct EmptyDocumentView: View {
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
                Text("Open a Markdown file, or open a folder to browse your workspace.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 360)
            }

            HStack(spacing: 12) {
                Button(action: openFile) {
                    Label("Open File…", systemImage: "doc")
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut("o", modifiers: .command)

                Button(action: openWorkspace) {
                    Label("Open Folder…", systemImage: "folder")
                }
                .buttonStyle(.bordered)
                .keyboardShortcut("o", modifiers: [.command, .shift])
            }
            .controlSize(.large)

            Text("Tip: drag a file onto the window, or press Space to peek at the source.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}
