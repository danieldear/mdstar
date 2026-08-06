import Foundation

/// Highlights and comments, persisted per document.
///
/// Keyed by the document's stable identifier from the Rust IR rather than by
/// path, so annotations survive a file being reopened.
@MainActor
final class AnnotationStore: ObservableObject {
    @Published private(set) var byDocument: [String: [Annotation]] = [:]

    private let defaults = UserDefaults.standard
    private let storageKey = "mdstar.native.annotations"

    init() {
        load()
    }

    func annotations(for documentID: String) -> [Annotation] {
        byDocument[documentID] ?? []
    }

    func annotations(for documentID: String, kind: Annotation.Kind) -> [Annotation] {
        annotations(for: documentID).filter { $0.kind == kind }
    }

    @discardableResult
    func add(
        kind: Annotation.Kind,
        range: NSRange,
        snippet: String,
        note: String = "",
        blockID: String? = nil,
        segments: [AnnotationSegment]? = nil,
        documentID: String
    ) -> Annotation {
        let annotation = Annotation(
            kind: kind, range: range, snippet: snippet, note: note, blockID: blockID,
            segments: segments
        )
        var existing = byDocument[documentID] ?? []
        existing.append(annotation)
        existing.sort { $0.location < $1.location }
        byDocument[documentID] = existing
        persist()
        return annotation
    }

    func updateNote(_ note: String, for id: UUID, documentID: String) {
        guard var existing = byDocument[documentID],
              let index = existing.firstIndex(where: { $0.id == id }) else { return }
        existing[index].note = note
        byDocument[documentID] = existing
        persist()
    }

    func remove(_ id: UUID, documentID: String) {
        guard var existing = byDocument[documentID] else { return }
        existing.removeAll { $0.id == id }
        if existing.isEmpty {
            byDocument.removeValue(forKey: documentID)
        } else {
            byDocument[documentID] = existing
        }
        persist()
    }

    func removeAll(for documentID: String) {
        byDocument.removeValue(forKey: documentID)
        persist()
    }

    // MARK: - Persistence

    private func persist() {
        guard let data = try? JSONEncoder().encode(byDocument) else { return }
        defaults.set(data, forKey: storageKey)
    }

    private func load() {
        guard let data = defaults.data(forKey: storageKey),
              let decoded = try? JSONDecoder().decode([String: [Annotation]].self, from: data) else {
            return
        }
        byDocument = decoded
    }
}
