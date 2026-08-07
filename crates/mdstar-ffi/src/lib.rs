//! Stable C ABI for native system adapters.
//!
//! HTML remains available for the existing Tauri frontend. New native clients
//! should consume the versioned semantic DocumentIR JSON API instead.

use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use serde::Serialize;

use mdstar_core::{document_ir::parse_document_ir_with_diagnostics, parse_markdown};
use mdstar_render_html::{base_stylesheet, render_document_ir, render_html};

const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_WORKSPACE_DEPTH: usize = 32;
const OPENABLE_EXTENSIONS: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "txt", "json", "yaml", "yml", "toml", "csv", "tsv", "xml",
];

#[derive(Serialize)]
struct FfiEnvelope<T: Serialize> {
    ok: bool,
    value: Option<T>,
    error: Option<FfiError>,
}

#[derive(Serialize)]
struct FfiError {
    code: String,
    message: String,
}

impl<T: Serialize> FfiEnvelope<T> {
    fn success(value: T) -> Self {
        Self {
            ok: true,
            value: Some(value),
            error: None,
        }
    }

    fn failure(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            value: None,
            error: Some(FfiError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WorkspaceNode {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub children: Vec<WorkspaceNode>,
}

/// Render markdown input to HTML and return an owned C string.
///
/// Compatibility surface for the Tauri frontend. New macOS code should call
/// `mdstar_document_ir_json` instead.
///
/// # Safety
/// `input` must be a valid, null-terminated UTF-8 C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_render_html(input: *const c_char) -> *mut c_char {
    ffi_string(|| {
        let markdown = required_utf8(input, "input").map_err(|error| error.message)?;
        let doc = parse_markdown(markdown).map_err(|error| error.to_string())?;
        Ok(render_html(&doc))
    })
}

/// Parse markdown into versioned semantic DocumentIR JSON.
///
/// `origin` should be a stable file path or URL. It scopes deterministic node
/// IDs used by native navigation and future annotation anchors.
///
/// # Safety
/// `input` and `origin` must be valid, null-terminated UTF-8 C string pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_document_ir_json(
    input: *const c_char,
    origin: *const c_char,
) -> *mut c_char {
    ffi_json(|| {
        let markdown = required_utf8(input, "input")?;
        let origin = required_utf8(origin, "origin")?;
        parse_document_ir_with_diagnostics(markdown, origin).map_err(|error| FfiError {
            code: "parse_failed".to_string(),
            message: error.to_string(),
        })
    })
}

/// Read a local document and return versioned semantic DocumentIR JSON.
///
/// # Safety
/// `path` must be a valid, null-terminated UTF-8 C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_document_ir_from_file_json(path: *const c_char) -> *mut c_char {
    ffi_json(|| {
        let path = required_utf8(path, "path")?;
        let metadata = fs::metadata(path).map_err(|error| FfiError {
            code: "file_unavailable".to_string(),
            message: format!("Cannot read {path}: {error}"),
        })?;
        if metadata.len() > MAX_DOCUMENT_BYTES {
            return Err(FfiError {
                code: "file_too_large".to_string(),
                message: format!(
                    "{path} exceeds the {} MiB native reader limit",
                    MAX_DOCUMENT_BYTES / 1024 / 1024
                ),
            });
        }
        let input = fs::read_to_string(path).map_err(|error| FfiError {
            code: "file_not_utf8".to_string(),
            message: format!("Cannot decode {path} as UTF-8 text: {error}"),
        })?;
        parse_document_ir_with_diagnostics(&input, path).map_err(|error| FfiError {
            code: "parse_failed".to_string(),
            message: error.to_string(),
        })
    })
}

/// Return a recursively sorted, supported-file workspace tree.
///
/// Symlinks are represented but never traversed, preventing cycles.
///
/// # Safety
/// `root` must be a valid, null-terminated UTF-8 C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_workspace_tree_json(root: *const c_char) -> *mut c_char {
    ffi_json(|| {
        let root = required_utf8(root, "root")?;
        scan_workspace(Path::new(root), 0).map_err(|error| FfiError {
            code: "workspace_scan_failed".to_string(),
            message: error.to_string(),
        })
    })
}

/// Read a local document and return sanitized semantic HTML for display.
///
/// The markup carries the IR's stable block identifiers so a frontend can map
/// scroll position, search hits and selections back to semantic blocks. Raw
/// HTML in the source is reduced to a presentational allowlist, because this
/// output is rendered in a web view.
///
/// # Safety
/// `path` must be a valid, null-terminated UTF-8 C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_document_html_from_file(path: *const c_char) -> *mut c_char {
    ffi_string(|| {
        let path = required_utf8(path, "path").map_err(|error| error.message)?;
        let input =
            fs::read_to_string(path).map_err(|error| format!("Cannot read {path}: {error}"))?;
        let ir =
            parse_document_ir_with_diagnostics(&input, path).map_err(|error| error.to_string())?;
        Ok(render_document_ir(&ir))
    })
}

/// Render in-memory Markdown to sanitized semantic HTML.
///
/// The file-based variant cannot show unsaved work, so an editor that wants a
/// live preview renders its buffer through this instead. `origin` scopes the
/// deterministic block identifiers and resolves relative links.
///
/// # Safety
/// `input` and `origin` must be valid, null-terminated UTF-8 C string pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_document_html(
    input: *const c_char,
    origin: *const c_char,
) -> *mut c_char {
    ffi_string(|| {
        let markdown = required_utf8(input, "input").map_err(|error| error.message)?;
        let origin = required_utf8(origin, "origin").map_err(|error| error.message)?;
        let ir = parse_document_ir_with_diagnostics(markdown, origin)
            .map_err(|error| error.to_string())?;
        Ok(render_document_ir(&ir))
    })
}

/// Structural stylesheet shared by every HTML consumer.
///
/// Colours and metrics are custom properties, so a frontend themes the document
/// by redefining variables rather than shipping its own copy of these rules.
///
/// Takes no arguments, so there is nothing for a caller to get wrong; the
/// returned string must still be released with `mdstar_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn mdstar_reader_stylesheet() -> *mut c_char {
    ffi_string(|| Ok(base_stylesheet().to_string()))
}

/// Free a string returned from any mdstar FFI function.
///
/// # Safety
/// `ptr` must have been returned by this library and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdstar_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: pointer ownership is transferred back from the FFI caller.
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

/// Backward-compatible free function name.
///
/// # Safety
/// Same as `mdstar_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn markdown_string_free(ptr: *mut c_char) {
    unsafe { mdstar_string_free(ptr) };
}

fn ffi_string(operation: impl FnOnce() -> Result<String, String>) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(operation))
        .unwrap_or_else(|_| Err("mdstar FFI recovered from an internal panic".to_string()));
    let output = match result {
        Ok(value) => value,
        Err(message) => message,
    };
    CString::new(output).map_or(std::ptr::null_mut(), CString::into_raw)
}

fn ffi_json<T: Serialize>(operation: impl FnOnce() -> Result<T, FfiError>) -> *mut c_char {
    let envelope = match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => FfiEnvelope::success(value),
        Ok(Err(error)) => FfiEnvelope::<T>::failure(error.code, error.message),
        Err(_) => FfiEnvelope::<T>::failure(
            "internal_panic",
            "mdstar FFI recovered from an internal panic",
        ),
    };
    match serde_json::to_string(&envelope) {
        Ok(json) => CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw),
        Err(_) => std::ptr::null_mut(),
    }
}

fn required_utf8<'a>(ptr: *const c_char, name: &str) -> Result<&'a str, FfiError> {
    if ptr.is_null() {
        return Err(FfiError {
            code: "invalid_argument".to_string(),
            message: format!("{name} must not be null"),
        });
    }
    // SAFETY: callers of the exported functions promise a null-terminated C string.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| FfiError {
            code: "invalid_utf8".to_string(),
            message: format!("{name} must be valid UTF-8"),
        })
}

fn scan_workspace(path: &Path, depth: usize) -> Result<WorkspaceNode, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    let is_symlink = metadata.file_type().is_symlink();
    let is_directory = metadata.is_dir() && !is_symlink;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("Workspace"))
        .to_string();

    let mut children = if is_directory && depth < MAX_WORKSPACE_DEPTH {
        fs::read_dir(path)?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                let entry_path = entry.path();
                let entry_metadata = fs::symlink_metadata(&entry_path).ok()?;
                let is_scannable_dir =
                    entry_metadata.is_dir() && !entry_metadata.file_type().is_symlink();
                let is_supported_doc = entry_metadata.is_file() && is_supported_file(&entry_path);
                if is_scannable_dir || is_supported_doc {
                    scan_workspace(&entry_path, depth + 1).ok()
                } else {
                    None
                }
            })
            .filter(|node| node.kind == "file" || !node.children.is_empty())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    children.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    Ok(WorkspaceNode {
        id: path.to_string_lossy().to_string(),
        name,
        path: path.to_string_lossy().to_string(),
        kind: if is_directory {
            "directory".to_string()
        } else {
            "file".to_string()
        },
        children,
    })
}

fn is_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|extension| {
            OPENABLE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn document_ir_ffi_returns_a_versioned_success_envelope() {
        let input = CString::new("# Hello\n\n- [x] done").unwrap();
        let origin = CString::new("file:///tmp/hello.md").unwrap();
        let pointer = unsafe { mdstar_document_ir_json(input.as_ptr(), origin.as_ptr()) };
        assert!(!pointer.is_null());
        let json = unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { mdstar_string_free(pointer) };
        assert!(json.contains("schema_version"));
        assert!(json.contains("\"ok\":true"));
    }

    #[test]
    fn invalid_arguments_return_json_errors_not_panics() {
        let origin = CString::new("file:///tmp/hello.md").unwrap();
        let pointer = unsafe { mdstar_document_ir_json(std::ptr::null(), origin.as_ptr()) };
        let json = unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { mdstar_string_free(pointer) };
        assert!(json.contains("invalid_argument"));
    }

    #[test]
    fn workspace_tree_is_directories_first_and_filters_binary_files() {
        let root = std::env::temp_dir().join(format!("mdstar-ffi-{}", std::process::id()));
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("guide.md"), "# Guide").unwrap();
        fs::write(root.join("zeta.txt"), "text").unwrap();
        fs::write(root.join("ignore.png"), "binary").unwrap();
        let tree = scan_workspace(&root, 0).unwrap();
        assert_eq!(tree.children[0].kind, "directory");
        assert_eq!(tree.children[0].name, "docs");
        assert!(tree.children.iter().all(|node| node.name != "ignore.png"));
        fs::remove_dir_all(root).unwrap();
    }
}
