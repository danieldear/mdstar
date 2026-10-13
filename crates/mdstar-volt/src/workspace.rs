//! Workspace folders: the Markdown files under each, as a tree.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::MARKDOWN;

/// Folders never worth showing in a document tree.
const SKIP: [&str; 14] = [
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    "vendor",
    "Pods",
    "DerivedData",
    "venv",
    "__pycache__",
    "site-packages",
    "bower_components",
    "coverage",
    "tmp",
];
const MAX_DEPTH: usize = 12;
/// Directory entries examined per scan, so a huge folder cannot stall.
const MAX_ENTRIES: usize = 20_000;

pub struct Node {
    pub path: PathBuf,
    pub name: String,
    /// `None` for a file.
    pub children: Option<Vec<Node>>,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.children.is_some()
    }
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MARKDOWN.contains(&e.to_ascii_lowercase().as_str()))
}

/// The Markdown files under `root`, keeping only folders that contain some.
/// Hidden entries and build or dependency folders are skipped.
pub fn scan(root: &Path) -> Node {
    let mut budget = MAX_ENTRIES;
    let children = scan_dir(root, 0, &mut budget).unwrap_or_default();
    Node {
        path: root.to_path_buf(),
        name: root
            .file_name()
            .map_or_else(|| root.display().to_string(), |n| n.to_string_lossy().into_owned()),
        children: Some(children),
    }
}

fn scan_dir(dir: &Path, depth: usize, budget: &mut usize) -> Option<Vec<Node>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // Follow symlinked files but not folders, which can loop.
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if SKIP.contains(&name.as_str()) {
                continue;
            }
            if let Some(children) = scan_dir(&path, depth + 1, budget)
                && !children.is_empty()
            {
                out.push(Node {
                    path,
                    name,
                    children: Some(children),
                });
            }
        } else if is_markdown(&path) && (kind.is_file() || path.is_file()) {
            out.push(Node {
                path,
                name,
                children: None,
            });
        }
    }
    out.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| natural(&a.name).cmp(&natural(&b.name)))
    });
    Some(out)
}

/// A sort key that orders "file 2" before "file 10", ignoring case.
fn natural(name: &str) -> Vec<(String, u64)> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    for c in name.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                parts.push((std::mem::take(&mut text), digits.parse().unwrap_or(u64::MAX)));
                digits.clear();
            }
            text.extend(c.to_lowercase());
        }
    }
    parts.push((text, digits.parse().unwrap_or(0)));
    parts
}

/// One visible row of the file tree.
pub struct Row<'a> {
    pub node: &'a Node,
    pub depth: usize,
    pub open: bool,
}

/// The rows shown for `roots`: expanded folders' contents, or with a
/// filter, every file whose name contains it and the folders above it.
pub fn rows<'a>(roots: &'a [Node], expanded: &HashSet<PathBuf>, filter: &str) -> Vec<Row<'a>> {
    let filter = filter.trim().to_lowercase();
    let mut out = Vec::new();
    for root in roots {
        if filter.is_empty() {
            visible(root, 0, expanded, &mut out);
        } else {
            matching(root, 0, &filter, &mut out);
        }
    }
    out
}

fn visible<'a>(node: &'a Node, depth: usize, expanded: &HashSet<PathBuf>, out: &mut Vec<Row<'a>>) {
    // Roots are always open.
    let open = depth == 0 || expanded.contains(&node.path);
    out.push(Row { node, depth, open });
    if let Some(children) = &node.children
        && open
    {
        for child in children {
            visible(child, depth + 1, expanded, out);
        }
    }
}

/// Push `node` if it or a descendant matches; returns whether it did.
fn matching<'a>(node: &'a Node, depth: usize, filter: &str, out: &mut Vec<Row<'a>>) -> bool {
    match &node.children {
        None => {
            let hit = node.name.to_lowercase().contains(filter);
            if hit {
                out.push(Row {
                    node,
                    depth,
                    open: false,
                });
            }
            hit
        }
        Some(children) => {
            let at = out.len();
            out.push(Row {
                node,
                depth,
                open: true,
            });
            let mut any = false;
            for child in children {
                any |= matching(child, depth + 1, filter, out);
            }
            if !any && depth > 0 {
                out.truncate(at);
            }
            any || depth == 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mdstar-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for file in [
            "README.md",
            "notes 10.md",
            "notes 2.md",
            "docs/guide.markdown",
            "docs/deep/api.md",
            "docs/image.png",
            "empty/readme.txt.bak",
            "node_modules/pkg/README.md",
            ".git/HEAD.md",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "# x").unwrap();
        }
        root
    }

    #[test]
    fn scans_markdown_only_and_sorts_folders_first_naturally() {
        let root = fixture("scan");
        let tree = scan(&root);
        let names: Vec<&str> = tree
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(names, ["docs", "notes 2.md", "notes 10.md", "README.md"]);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rows_follow_expansion_and_filters_keep_parent_folders() {
        let root = fixture("rows");
        let tree = [scan(&root)];
        let names = |rows: Vec<Row>| -> Vec<String> {
            rows.iter()
                .map(|r| format!("{}{}", "  ".repeat(r.depth), r.node.name))
                .collect()
        };
        let collapsed = names(rows(&tree, &HashSet::new(), ""));
        assert_eq!(collapsed.len(), 5, "{collapsed:?}");
        let expanded: HashSet<PathBuf> = [root.join("docs")].into();
        assert_eq!(rows(&tree, &expanded, "").len(), 7);
        let filtered = names(rows(&tree, &HashSet::new(), "API"));
        assert_eq!(filtered[1..], ["  docs", "    deep", "      api.md"]);
        std::fs::remove_dir_all(root).ok();
    }
}
