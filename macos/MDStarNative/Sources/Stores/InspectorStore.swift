import Foundation

enum InspectorSection: String, CaseIterable, Identifiable {
    case bookmarks = "Bookmarks"
    case highlights = "Highlights"
    case comments = "Comments"

    var id: Self { self }

    var systemImage: String {
        switch self {
        case .bookmarks: "bookmark"
        case .highlights: "highlighter"
        case .comments: "text.bubble"
        }
    }

    var emptyMessage: String {
        switch self {
        case .bookmarks: "No bookmarks in this document."
        case .highlights: "No highlights in this document."
        case .comments: "No comments in this document."
        }
    }
}

@MainActor
final class InspectorStore: ObservableObject {
    @Published var isVisible = false
    @Published var selectedSection: InspectorSection = .bookmarks
}
