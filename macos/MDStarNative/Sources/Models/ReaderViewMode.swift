import Foundation

/// The reader's presentation mode. `split` pairs a read-only source pane with
/// the rendered document; `view` is the rendered document alone.
enum ReaderViewMode: String, CaseIterable, Identifiable {
    case split
    case view

    var id: String { rawValue }

    var title: String {
        switch self {
        case .split: "Split"
        case .view: "View"
        }
    }

    var systemImage: String {
        switch self {
        case .split: "rectangle.split.2x1"
        case .view: "book"
        }
    }

    var showsSource: Bool { self == .split }
    var showsRendered: Bool { true }
}
