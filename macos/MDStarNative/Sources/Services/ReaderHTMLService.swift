import Foundation

@_silgen_name("mdstar_document_html_from_file")
private func mdstarDocumentHTMLFromFile(_ path: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("mdstar_document_html")
private func mdstarDocumentHTML(
    _ input: UnsafePointer<CChar>,
    _ origin: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>?

@_silgen_name("mdstar_reader_stylesheet")
private func mdstarReaderStylesheet() -> UnsafeMutablePointer<CChar>?

@_silgen_name("mdstar_string_free")
private func mdstarStringFree(_ pointer: UnsafeMutablePointer<CChar>?)

/// Bridges the Rust HTML renderer.
///
/// Markup and the structural stylesheet both come from the core so the UI and
/// console frontends stay in step; Swift only supplies theming on top.
struct ReaderHTMLService: Sendable {
    /// Sanitized semantic HTML for a document, carrying stable block ids.
    func html(forFileAt url: URL) -> String? {
        url.path.withCString { pointer in
            guard let result = mdstarDocumentHTMLFromFile(pointer) else { return nil }
            defer { mdstarStringFree(result) }
            return String(cString: result)
        }
    }

    /// HTML for in-memory Markdown. Used for the live preview, where the file
    /// on disk does not yet reflect what is being edited.
    func html(forSource source: String, origin: String) -> String? {
        source.withCString { sourcePointer in
            origin.withCString { originPointer in
                guard let result = mdstarDocumentHTML(sourcePointer, originPointer) else {
                    return nil
                }
                defer { mdstarStringFree(result) }
                return String(cString: result)
            }
        }
    }

    /// Structural CSS shared by every HTML consumer.
    func baseStylesheet() -> String {
        guard let result = mdstarReaderStylesheet() else { return "" }
        defer { mdstarStringFree(result) }
        return String(cString: result)
    }
}
