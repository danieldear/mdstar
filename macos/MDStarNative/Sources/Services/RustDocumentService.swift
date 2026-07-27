import Foundation

@_silgen_name("mdstar_document_ir_from_file_json")
private func mdstarDocumentIRFromFileJSON(_ path: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("mdstar_workspace_tree_json")
private func mdstarWorkspaceTreeJSON(_ root: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("mdstar_string_free")
private func mdstarStringFree(_ pointer: UnsafeMutablePointer<CChar>?)

enum RustDocumentServiceError: LocalizedError {
    case invalidResponse
    case ffi(String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse: "MD Star did not return a readable native document response."
        case .ffi(let message): message
        }
    }
}

struct RustDocumentService: Sendable {

    func loadDocument(at url: URL) throws -> DocumentIR {
        let json = try ffiString(url.path) { pointer in
            mdstarDocumentIRFromFileJSON(pointer)
        }
        let envelope = try JSONDecoder().decode(FFIEnvelope<DocumentIR>.self, from: Data(json.utf8))
        guard envelope.ok, let document = envelope.value else {
            throw envelope.error ?? RustDocumentServiceError.invalidResponse
        }
        return document
    }

    func workspaceTree(at url: URL) throws -> FileNode {
        let json = try ffiString(url.path) { pointer in
            mdstarWorkspaceTreeJSON(pointer)
        }
        let envelope = try JSONDecoder().decode(FFIEnvelope<FileNode>.self, from: Data(json.utf8))
        guard envelope.ok, let tree = envelope.value else {
            throw envelope.error ?? RustDocumentServiceError.invalidResponse
        }
        return tree
    }

    private func ffiString(
        _ value: String,
        call: (UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
    ) throws -> String {
        try value.withCString { pointer in
            guard let result = call(pointer) else { throw RustDocumentServiceError.invalidResponse }
            defer { mdstarStringFree(result) }
            return String(cString: result)
        }
    }
}
