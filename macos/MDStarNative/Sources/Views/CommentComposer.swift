import SwiftUI

/// Sheet for attaching a note to the selected passage.
struct CommentComposer: View {
    let snippet: String
    @Binding var note: String
    let onCancel: () -> Void
    let onSave: () -> Void

    @FocusState private var noteFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add Comment")
                .font(.headline)

            VStack(alignment: .leading, spacing: 6) {
                Text("Selected text")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(snippet)
                    .font(.callout)
                    .lineLimit(4)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.readerSubtleFill, in: RoundedRectangle(cornerRadius: 7))
            }

            VStack(alignment: .leading, spacing: 6) {
                Text("Note")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextEditor(text: $note)
                    .font(.body)
                    .frame(height: 100)
                    .padding(4)
                    .overlay(
                        RoundedRectangle(cornerRadius: 7).stroke(Color.readerHairline)
                    )
                    .focused($noteFocused)
            }

            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Add Comment", action: onSave)
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 420)
        .onAppear { noteFocused = true }
    }
}
