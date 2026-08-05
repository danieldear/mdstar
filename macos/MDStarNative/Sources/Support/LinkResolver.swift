import Foundation

/// Resolves link and image targets found in a document.
///
/// Rendering itself belongs to the Rust core, which emits the HTML; the app
/// only needs to work out where a link points when the page reports a click.
enum LinkResolver {
    /// URL scheme marking an in-page anchor (`[x](#heading)`), so a click can be
    /// answered by scrolling rather than by navigating.
    static let anchorScheme = "mdstaranchor"

    /// The heading slug carried by an anchor URL, if it is one.
    static func anchorSlug(from url: URL) -> String? {
        guard url.scheme == anchorScheme else { return nil }
        let raw = url.absoluteString.dropFirst("\(anchorScheme):".count)
        return raw.removingPercentEncoding ?? String(raw)
    }

    /// Web URLs pass through; local references are percent-decoded, tilde
    /// expanded, and resolved against the document's directory.
    static func resolvedURL(_ raw: String?, origin: String) -> URL? {
        guard let raw, !raw.isEmpty else { return nil }

        if raw.hasPrefix("#") {
            let slug = String(raw.dropFirst()).lowercased()
            let encoded = slug.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? slug
            return URL(string: "\(anchorScheme):\(encoded)")
        }

        if let absolute = URL(string: raw), let scheme = absolute.scheme, scheme != "file" {
            return absolute
        }

        if raw.hasPrefix("file://"), let fileURL = URL(string: raw) {
            return fileURL.standardizedFileURL
        }

        // Markdown may percent-encode spaces, so decode before touching the path.
        let path = raw.removingPercentEncoding ?? raw

        if path.hasPrefix("~") {
            return URL(fileURLWithPath: (path as NSString).expandingTildeInPath).standardizedFileURL
        }
        if path.hasPrefix("/") {
            return URL(fileURLWithPath: path).standardizedFileURL
        }

        guard origin.hasPrefix("/") || origin.hasPrefix("file://") else { return URL(string: raw) }
        let originURL = origin.hasPrefix("file://") ? URL(string: origin) : URL(fileURLWithPath: origin)
        guard let base = originURL?.deletingLastPathComponent() else { return URL(string: raw) }
        return URL(fileURLWithPath: path, relativeTo: base).standardizedFileURL
    }
}
