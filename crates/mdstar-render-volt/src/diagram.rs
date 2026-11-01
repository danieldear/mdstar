//! Mermaid diagrams drawn natively.
//!
//! `mermaid-text` parses the source. Flowcharts and state diagrams then get
//! a layered layout in pixels (layers, crossing reduction, least-squares
//! placement) and are drawn as vector shapes with smooth edges; sequence
//! diagrams get lanes, messages, notes and blocks; pie charts use the kit's
//! donut chart. Class and ER diagrams prefer the extended renderer below.
//!
//! Extended families (Gantt, timeline, mindmap, journey, etc.) use the same
//! strict Rust SVG renderer as Volt Editor, rasterized offline into a bounded
//! high-resolution image. No scripts or external image resources are loaded.
//! Layouts are cached by source, so rebuilding a view does not redo them.

use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    rc::Rc,
};

use mermaid_text::{
    Direction, EdgeEndpoint, EdgeStyle, NodeShape,
    detect::{DiagramKind, detect},
    sequence::{NoteAnchor, SequenceDiagram},
    types::{BarOrientation, Rgb},
};
use volt_ui_kit::{
    graphics::{Image, VectorPath},
    ui::prelude::*,
};

/// Label font size and line height the layout is computed at.
const FONT: f32 = 13.;
const LINE: f32 = 17.;
const PAD_X: f32 = 14.;
const PAD_Y: f32 = 9.;
/// Space between nodes in a layer, and between layers.
const NODE_GAP: f32 = 34.;
const LAYER_GAP: f32 = 46.;
/// With edge labels every edge passes a label layer; gaps either side.
const HALF_GAP: f32 = 22.;
const EDGE_FONT: f32 = 12.;
const MARGIN: f32 = 6.;

pub enum Diagram {
    Pending,
    Graph(Graph),
    Sequence(Sequence),
    Pie {
        title: Option<String>,
        slices: Vec<(String, f32)>,
    },
    /// `mermaid-text`'s Unicode drawing, for kinds without native drawing.
    Text(String),
    Image {
        image: Image,
        size: [f32; 2],
    },
    Error(String),
}

impl std::fmt::Debug for Diagram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Diagram")
            .field("size", &size(self))
            .finish_non_exhaustive()
    }
}
// Live rendering uses one bounded worker; synchronous rendering is reserved
// for explicit export and the legacy non-UI helper.
type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;
const PREVIEW_BYTES: usize = 32 * 1024 * 1024;
#[derive(Default)]
struct PreviewCache {
    ready: HashMap<String, std::sync::Arc<Diagram>>,
    order: std::collections::VecDeque<String>,
    pending: std::collections::HashSet<String>,
    mounted: std::collections::HashSet<String>,
}
struct PreviewWorker {
    jobs: std::sync::mpsc::SyncSender<String>,
    cache: std::sync::Arc<std::sync::Mutex<PreviewCache>>,
}
static PREVIEWS: std::sync::OnceLock<Option<PreviewWorker>> = std::sync::OnceLock::new();
static WAKE: std::sync::Mutex<Option<Wake>> = std::sync::Mutex::new(None);
pub fn set_waker(wake: Wake) {
    *WAKE.lock().unwrap_or_else(|e| e.into_inner()) = Some(wake.clone());
    if PREVIEWS.get().and_then(Option::as_ref).is_some_and(|w| {
        !w.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ready
            .is_empty()
    }) {
        wake();
    }
}
fn preview_weight(source: &str, d: &Diagram) -> usize {
    // Include a conservative native-layout allowance, not just RGBA pixels.
    source.len()
        + match d {
            Diagram::Image { image, .. } => image.pixels().len(),
            Diagram::Error(_) | Diagram::Pending => 256,
            _ => source.len().saturating_mul(64),
        }
}
fn previews() -> Option<&'static PreviewWorker> {
    PREVIEWS.get_or_init(|| {
        let (jobs, rx) = std::sync::mpsc::sync_channel::<String>(8);
        let cache = std::sync::Arc::new(std::sync::Mutex::new(PreviewCache::default()));
        let sink = cache.clone();
        std::thread::Builder::new().name("mdstar-diagrams".into()).spawn(move || {
            while let Ok(source) = rx.recv() {
                let mut drawing = std::panic::catch_unwind(|| build(&source))
                    .unwrap_or_else(|_| Diagram::Error("Diagram renderer could not process this source".into()));
                let mut c = sink.lock().unwrap_or_else(|e| e.into_inner());
                let mut weight = c.ready.iter().map(|(s, d)| preview_weight(s, d)).sum::<usize>();
                let wanted = preview_weight(&source, &drawing);
                while c.ready.len() >= 64 || weight.saturating_add(wanted) > PREVIEW_BYTES {
                    let victim = c.order.iter().position(|s| !c.mounted.contains(s));
                    let Some(i) = victim else { break; };
                    if let Some(key) = c.order.remove(i)
                        && let Some(d) = c.ready.remove(&key) {
                        weight = weight.saturating_sub(preview_weight(&key, &d));
                    }
                }
                if c.ready.len() >= 64 {
                    c.pending.remove(&source); drop(c);
                    if let Some(wake) = WAKE.lock().unwrap_or_else(|e| e.into_inner()).clone() { wake(); }
                    continue;
                }
                if weight.saturating_add(wanted) > PREVIEW_BYTES {
                    // Do not evict currently visible results and regenerate them
                    // on every frame; retain an explicit source/error fallback.
                    drawing = Diagram::Error("Visible diagram previews exceed the preview budget; source is retained".into());
                }
                c.pending.remove(&source); c.order.push_back(source.clone());
                c.ready.insert(source, std::sync::Arc::new(drawing));
                drop(c);
                let wake = WAKE.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if let Some(wake) = wake { wake(); }
            }
        }).ok()?;
        Some(PreviewWorker { jobs, cache })
    }).as_ref()
}
pub fn begin_frame() {
    if let Some(Some(w)) = PREVIEWS.get() {
        w.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mounted
            .clear();
    }
}
pub fn peek(source: &str) -> Option<std::sync::Arc<Diagram>> {
    PREVIEWS
        .get()?
        .as_ref()?
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ready
        .get(source)
        .cloned()
}
pub fn request(source: &str) -> std::sync::Arc<Diagram> {
    if source.len() > 64 * 1024 {
        return std::sync::Arc::new(Diagram::Error(
            "Diagram exceeds the 64 KiB source limit".into(),
        ));
    }
    let Some(w) = previews() else {
        return std::sync::Arc::new(Diagram::Error("Could not start diagram worker".into()));
    };
    let mut c = w.cache.lock().unwrap_or_else(|e| e.into_inner());
    if c.mounted.len() >= 64 && !c.mounted.contains(source) {
        return std::sync::Arc::new(Diagram::Error(
            "Too many mounted diagram previews; source is retained".into(),
        ));
    }
    c.mounted.insert(source.to_owned());
    if let Some(d) = c.ready.get(source) {
        return d.clone();
    }
    if !c.pending.contains(source)
        && c.pending.len() < 8
        && w.jobs.try_send(source.to_owned()).is_ok()
    {
        c.pending.insert(source.to_owned());
    }
    std::sync::Arc::new(Diagram::Pending)
}

/// The diagram for a Mermaid source, laid out once and cached.
pub fn diagram(source: &str) -> Rc<Diagram> {
    thread_local! {
        static CACHE: RefCell<HashMap<u64, Rc<Diagram>>> = RefCell::new(HashMap::new());
    }
    let mut h = DefaultHasher::new();
    source.hash(&mut h);
    let key = h.finish();
    CACHE.with(|cache| {
        if let Some(d) = cache.borrow().get(&key) {
            return d.clone();
        }
        let d = Rc::new(build(source));
        let mut cache = cache.borrow_mut();
        let image_bytes: usize = cache
            .values()
            .map(|d| match d.as_ref() {
                Diagram::Image { image, .. } => image.pixels().len(),
                _ => 0,
            })
            .sum();
        if cache.len() >= 64 || image_bytes >= 32 * 1024 * 1024 {
            cache.clear();
        }
        cache.insert(key, d.clone());
        d
    })
}

pub fn build(source: &str) -> Diagram {
    build_bounded(source, 256, 512)
}
/// Same engine with a host's stricter resource budget, checked before layout.
pub fn build_bounded(source: &str, max_nodes: usize, max_edges: usize) -> Diagram {
    if source.len() > 64 * 1024 {
        return Diagram::Error("Diagram exceeds the 64 KiB source limit".into());
    }
    if source.lines().count() > 1024 {
        return Diagram::Error("Diagram exceeds the 1024-line source limit".into());
    }
    let mut depth = 0usize;
    for byte in source.bytes() {
        match byte {
            b'{' | b'[' | b'(' => {
                depth += 1;
                if depth > 48 {
                    return Diagram::Error("Diagram nesting exceeds the safe limit".into());
                }
            }
            b'}' | b']' | b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let native = build_native(source, max_nodes, max_edges);
    if matches!(native, Diagram::Error(_) | Diagram::Text(_)) {
        match extended(source, max_nodes, max_edges) {
            Ok(d) => d,
            // Retain existing text output when a legacy family cannot be
            // converted; invalid input keeps the source/error card.
            Err(e) if matches!(native, Diagram::Error(_)) => Diagram::Error(e),
            Err(_) => native,
        }
    } else {
        native
    }
}

/// Match Volt Editor's strict parser, but keep raster work and cached memory
/// bounded. A white canvas preserves label contrast even in a dark reader.
fn extended(source: &str, max_nodes: usize, max_edges: usize) -> Result<Diagram, String> {
    use resvg::{tiny_skia, usvg};
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|e| e.to_string())?;
    if parsed.graph.nodes.len() > max_nodes || parsed.graph.edges.len() > max_edges {
        return Err("Diagram exceeds the safe node/edge limit".into());
    }
    if parsed.graph.gantt_tasks.len() > 256 {
        return Err("Diagram exceeds the safe task limit".into());
    }
    if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Gantt
        && parsed.graph.gantt_tasks.is_empty()
    {
        return Err("Gantt chart contains no valid tasks".into());
    }
    let theme = mermaid_rs_renderer::Theme::modern();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &Default::default());
    let svg = mermaid_rs_renderer::render_svg(&layout, &theme, &Default::default());
    if svg.len() > 2 * 1024 * 1024 || svg.contains("<foreignObject") || svg.contains("<image") {
        return Err("Diagram contains unsupported or oversized SVG content".into());
    }
    thread_local! {
        static FONTS: std::sync::Arc<usvg::fontdb::Database> = {
            let mut fonts = usvg::fontdb::Database::new();
            fonts.load_system_fonts();
            remove_placeholder_fonts(&mut fonts);
            std::sync::Arc::new(fonts)
        };
    }
    let options = usvg::Options {
        fontdb: FONTS.with(Clone::clone),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    let size = [tree.size().width(), tree.size().height()];
    if size
        .iter()
        .any(|v| !v.is_finite() || *v <= 0. || *v > 20_000.)
    {
        return Err("Invalid or oversized diagram dimensions".into());
    }
    let scale = 2_f32.min((4_000_000. / (size[0] * size[1])).sqrt());
    let mut pixels = tiny_skia::Pixmap::new(
        (size[0] * scale).floor().max(1.) as u32,
        (size[1] * scale).floor().max(1.) as u32,
    )
    .ok_or("Could not allocate diagram image")?;
    pixels.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    // Opaque white background means these premultiplied pixels are also
    // straight RGBA, as required by Image::from_rgba.
    let image =
        Image::from_rgba(pixels.width(), pixels.height(), pixels.take()).map_err(str::to_owned)?;
    Ok(Diagram::Image { image, size })
}

// Geometry/labels come from the strict parser, never the terminal parser's
// invented nodes for fan-out syntax (e.g. `A & B --> C`). Preserve the latter's
// styling/nested-group metadata for the native layered drawing.
fn strict_flowchart(source: &str) -> Result<mermaid_text::Graph, String> {
    use mermaid_rs_renderer::ir;
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|e| e.to_string())?;
    let strict = parsed.graph;
    let direction = match strict.direction {
        ir::Direction::TopDown => Direction::TopToBottom,
        ir::Direction::BottomTop => Direction::BottomToTop,
        ir::Direction::LeftRight => Direction::LeftToRight,
        ir::Direction::RightLeft => Direction::RightToLeft,
    };
    let mut graph = mermaid_text::parser::flowchart::parse(source).map_err(|e| e.to_string())?;
    graph.direction = direction;
    let mut nodes: Vec<_> = strict.nodes.values().collect();
    nodes.sort_by_key(|n| strict.node_order.get(&n.id).copied().unwrap_or(usize::MAX));
    graph.nodes = nodes
        .into_iter()
        .map(|n| {
            let shape = match n.shape {
                ir::NodeShape::RoundRect => NodeShape::Rounded,
                ir::NodeShape::Stadium => NodeShape::Stadium,
                ir::NodeShape::Subroutine => NodeShape::Subroutine,
                ir::NodeShape::Cylinder => NodeShape::Cylinder,
                ir::NodeShape::Circle => NodeShape::Circle,
                ir::NodeShape::DoubleCircle => NodeShape::DoubleCircle,
                ir::NodeShape::Diamond => NodeShape::Diamond,
                ir::NodeShape::Hexagon => NodeShape::Hexagon,
                ir::NodeShape::Parallelogram | ir::NodeShape::ParallelogramAlt => {
                    NodeShape::Parallelogram
                }
                ir::NodeShape::Trapezoid | ir::NodeShape::TrapezoidAlt => NodeShape::Trapezoid,
                ir::NodeShape::Asymmetric => NodeShape::Asymmetric,
                ir::NodeShape::ForkJoin => NodeShape::Bar(
                    if matches!(direction, Direction::LeftToRight | Direction::RightToLeft) {
                        BarOrientation::Vertical
                    } else {
                        BarOrientation::Horizontal
                    },
                ),
                _ => NodeShape::Rectangle,
            };
            mermaid_text::Node::new(&n.id, &n.label, shape)
        })
        .collect();
    let endpoint = |decoration: Option<ir::EdgeDecoration>, arrow: bool| match decoration {
        Some(ir::EdgeDecoration::Circle) => EdgeEndpoint::Circle,
        Some(ir::EdgeDecoration::Cross) => EdgeEndpoint::Cross,
        _ if arrow => EdgeEndpoint::Arrow,
        _ => EdgeEndpoint::None,
    };
    graph.edges = strict
        .edges
        .iter()
        .map(|e| mermaid_text::Edge {
            from: e.from.clone(),
            to: e.to.clone(),
            label: e.label.clone(),
            style: match e.style {
                ir::EdgeStyle::Solid => EdgeStyle::Solid,
                ir::EdgeStyle::Dotted => EdgeStyle::Dotted,
                ir::EdgeStyle::Thick => EdgeStyle::Thick,
            },
            start: endpoint(e.start_decoration, e.arrow_start),
            end: endpoint(e.end_decoration, e.arrow_end),
        })
        .collect();
    // Class colours and explicit style directives resolve through one parser.
    let rgb = |v: &Option<String>| {
        v.as_deref()
            .filter(|v| v.is_ascii())
            .and_then(Rgb::parse_hex)
    };
    for node in &graph.nodes {
        let mut style = mermaid_text::types::NodeStyle::default();
        if let Some(classes) = strict.node_classes.get(&node.id) {
            for class in classes {
                if let Some(s) = strict.class_defs.get(class) {
                    style.fill = rgb(&s.fill).or(style.fill);
                    style.stroke = rgb(&s.stroke).or(style.stroke);
                    style.color = rgb(&s.text_color).or(style.color);
                }
            }
        }
        if let Some(s) = strict.node_styles.get(&node.id) {
            style.fill = rgb(&s.fill).or(style.fill);
            style.stroke = rgb(&s.stroke).or(style.stroke);
            style.color = rgb(&s.text_color).or(style.color);
        }
        graph.node_styles.insert(node.id.clone(), style);
    }
    for group in &mut graph.subgraphs {
        if let Some(s) = strict
            .subgraphs
            .iter()
            .find(|s| s.id.as_deref() == Some(&group.id))
        {
            group.label = s.label.trim_matches('"').to_owned();
        } else {
            group.label = group.label.trim_matches('"').to_owned();
        }
    }
    Ok(graph)
}

fn build_native(source: &str, max_nodes: usize, max_edges: usize) -> Diagram {
    let kind = match detect(source) {
        Ok(kind) => kind,
        Err(e) => return Diagram::Error(e.to_string()),
    };
    let parsed = match kind {
        DiagramKind::Flowchart => strict_flowchart(source).map(|g| {
            if g.nodes.len() > max_nodes || g.edges.len() > max_edges {
                return Diagram::Error("Diagram exceeds the safe node/edge limit".into());
            }
            Diagram::Graph(layout(&g))
        }),
        DiagramKind::State => mermaid_text::parser::state::parse(source)
            .map_err(|e| e.to_string())
            .map(|mut g| {
                if g.nodes.len() > max_nodes || g.edges.len() > max_edges {
                    return Diagram::Error("Diagram exceeds the safe node/edge limit".into());
                }
                // The parser defaults to left-to-right for terminals; Mermaid
                // draws state diagrams top to bottom unless told otherwise.
                if !source
                    .lines()
                    .any(|l| l.trim_start().starts_with("direction "))
                {
                    g.direction = Direction::TopToBottom;
                    for n in &mut g.nodes {
                        if let NodeShape::Bar(o) = &mut n.shape {
                            *o = BarOrientation::Horizontal;
                        }
                    }
                }
                Diagram::Graph(layout(&g))
            }),
        DiagramKind::Sequence => mermaid_text::parser::sequence::parse(source)
            .map_err(|e| e.to_string())
            .map(|s| {
                if s.participants.len() > max_nodes.min(128)
                    || s.messages.len() > max_edges.min(512)
                {
                    return Diagram::Error(
                        "Sequence exceeds the safe participant/message limit".into(),
                    );
                }
                Diagram::Sequence(sequence(&s))
            }),
        DiagramKind::Pie => mermaid_text::parser::pie::parse(source)
            .map_err(|e| e.to_string())
            .map(|p| Diagram::Pie {
                title: p.title.clone(),
                slices: p
                    .slices
                    .iter()
                    .map(|s| (s.label.clone(), s.value as f32))
                    .collect(),
            }),
        _ => mermaid_text::render(source)
            .map(Diagram::Text)
            .map_err(|e| e.to_string()),
    };
    parsed.unwrap_or_else(|e| Diagram::Error(e.to_string()))
}

/// The natural size of a drawn diagram (text diagrams excluded).
/// Bounded cache accounting for host caches (not just vector paths).
pub fn memory_cost(d: &Diagram) -> usize {
    match d {
        Diagram::Image { image, .. } => image.pixels().len(),
        Diagram::Text(t) | Diagram::Error(t) => t.len() + 256,
        Diagram::Graph(g) => {
            g.nodes
                .iter()
                .map(|n| n.lines.iter().map(String::len).sum::<usize>() + 512)
                .sum::<usize>()
                + g.edges
                    .iter()
                    .map(|e| e.curves.len() * 64 + 512)
                    .sum::<usize>()
        }
        Diagram::Sequence(s) => {
            s.actors
                .iter()
                .map(|(_, _, labels)| labels.iter().map(String::len).sum::<usize>() + 128)
                .sum::<usize>()
                + (s.messages.len() + s.notes.len() + s.frames.len()) * 1024
                + s.bars.len() * 32
        }
        Diagram::Pie { slices, .. } => slices.len() * 128 + 256,
        Diagram::Pending => 256,
    }
}
fn remove_placeholder_fonts(fonts: &mut resvg::usvg::fontdb::Database) {
    let ids: Vec<_> = fonts
        .faces()
        .filter(|f| {
            f.post_script_name
                .to_ascii_lowercase()
                .contains("lastresort")
                || f.families
                    .iter()
                    .any(|(name, _)| name.to_ascii_lowercase().contains("lastresort"))
        })
        .map(|f| f.id)
        .collect();
    for id in ids {
        fonts.remove_face(id);
    }
}

pub fn size(d: &Diagram) -> Option<[f32; 2]> {
    match d {
        Diagram::Image { size, .. } => Some(*size),
        Diagram::Graph(g) => Some(g.size),
        Diagram::Sequence(s) => Some(s.size),
        Diagram::Pie { title, .. } => Some([360., 200. + if title.is_some() { 30. } else { 0. }]),
        Diagram::Pending | Diagram::Text(_) | Diagram::Error(_) => None,
    }
}

/// The smallest scale a diagram shrinks to: its 11 px labels stay at the
/// 8 px the kit allows and still read. Wider diagrams scroll instead.
pub const MIN_SCALE: f32 = 0.75;

/// The scale a diagram is drawn at: the reading size, shrunk to fit `room`.
pub fn scale(d: &Diagram, body: f32, room: f32) -> f32 {
    let z = (body / 16.).clamp(0.8, 1.4);
    match size(d) {
        Some([w, _]) if w * z > room => (room / w).max(MIN_SCALE),
        _ => z,
    }
}

/// Approximate advance of `text` in the system font at `size` px: widths
/// from a Helvetica-like table, a little generous so labels never overflow.
pub fn text_width(text: &str, size: f32) -> f32 {
    let em: f32 = text
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '|' | '\'' | '!' | '.' | ',' | ':' | ';' => 0.27,
            'f' | 't' | 'r' | 'I' | ' ' | '(' | ')' | '[' | ']' | '-' | '/' | '"' => 0.35,
            'm' | 'M' | 'W' | 'w' | '@' | '%' => 0.88,
            'A'..='Z' => 0.7,
            '0'..='9' => 0.59,
            'a'..='z' => 0.56,
            c if c.is_ascii() => 0.58,
            c if u32::from(c) >= 0x2E80 => 1.02,
            _ => 0.62,
        })
        .sum();
    em * size * 1.04
}

/// A label's lines: Mermaid's `<br>` tags and `\n` both break.
fn lines(label: &str) -> Vec<String> {
    let mut s = label.to_owned();
    for tag in ["<br/>", "<br />", "<br>", "<BR>", "<BR/>"] {
        s = s.replace(tag, "\n");
    }
    s.split('\n').map(|l| l.trim().to_owned()).collect()
}

fn block_size(lines: &[String], size: f32, line: f32) -> [f32; 2] {
    let w = lines.iter().map(|l| text_width(l, size)).fold(0., f32::max);
    [w, lines.len() as f32 * line]
}

fn hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

// ── Graphs ──────────────────────────────────────────────────────────────

pub struct Graph {
    pub size: [f32; 2],
    nodes: Vec<Shape>,
    edges: Vec<Wire>,
    clusters: Vec<Cluster>,
}

struct Shape {
    /// x, y, width, height.
    rect: [f32; 4],
    kind: NodeShape,
    lines: Vec<String>,
    fill: Option<String>,
    stroke: Option<String>,
    color: Option<String>,
}

struct Wire {
    /// Start point, then (control, control, end) per cubic segment.
    start: [f32; 2],
    curves: Vec<[[f32; 2]; 3]>,
    style: EdgeStyle,
    head: EdgeEndpoint,
    tail: EdgeEndpoint,
    label: Option<(Vec<String>, [f32; 4])>,
    color: Option<String>,
}

struct Cluster {
    rect: [f32; 4],
    label: String,
}

/// State diagrams mark start and end with ● and ◉ labels.
fn marker(kind: &NodeShape, label: &str) -> Option<bool> {
    match kind {
        NodeShape::Circle if label == "●" || label.is_empty() => Some(true),
        NodeShape::DoubleCircle if label == "◉" || label == "●" || label.is_empty() => {
            Some(false)
        }
        _ => None,
    }
}

/// Width and height of a node drawn around `lines`.
fn node_size(kind: &NodeShape, label: &str, lines: &[String]) -> [f32; 2] {
    if let Some(start) = marker(kind, label) {
        return if start { [16., 16.] } else { [20., 20.] };
    }
    let [tw, th] = block_size(lines, FONT, LINE);
    let (w, h) = (tw + 2. * PAD_X, th + 2. * PAD_Y);
    match kind {
        NodeShape::Diamond => {
            let s = (tw + th + 26.).max(52.);
            [s, s]
        }
        NodeShape::Circle | NodeShape::DoubleCircle => {
            let d = (tw.max(th) + 26.).max(44.);
            [d, d]
        }
        NodeShape::Stadium => [w + h * 0.5, h],
        NodeShape::Hexagon => [w + h * 0.6, h],
        NodeShape::Parallelogram | NodeShape::Trapezoid | NodeShape::Asymmetric => [w + 22., h],
        NodeShape::Cylinder => [w.max(56.), h + 14.],
        NodeShape::Subroutine => [w + 14., h],
        NodeShape::Bar(BarOrientation::Vertical) => [8., 70.],
        NodeShape::Bar(BarOrientation::Horizontal) => [70., 8.],
        _ => [w.max(48.), h.max(36.)],
    }
}

/// An edge label: the waypoint carrying it, its lines and its size.
type EdgeLabel = (usize, Vec<String>, [f32; 2]);

/// One vertex of the layered graph: a node or an edge's waypoint.
struct Vertex {
    /// Size across the layer (breadth) and along the flow (length).
    breadth: f32,
    length: f32,
    real: bool,
    /// An edge label rides on this waypoint.
    label: bool,
}

pub fn layout(g: &mermaid_text::Graph) -> Graph {
    let n = g.nodes.len();
    let index: HashMap<&str, usize> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, v)| (v.id.as_str(), i))
        .collect();
    let across = matches!(g.direction, Direction::LeftToRight | Direction::RightToLeft);
    let labels: Vec<Vec<String>> = g.nodes.iter().map(|v| lines(&v.label)).collect();
    let sizes: Vec<[f32; 2]> = g
        .nodes
        .iter()
        .zip(&labels)
        .map(|(v, l)| node_size(&v.shape, &v.label, l))
        .collect();
    // (breadth, length) in flow space.
    let flow = |s: [f32; 2]| if across { [s[1], s[0]] } else { s };

    // Edges between known nodes; self-loops are drawn separately.
    let edges: Vec<(usize, usize, usize)> = g
        .edges
        .iter()
        .enumerate()
        .filter_map(|(e, edge)| {
            Some((
                *index.get(edge.from.as_str())?,
                *index.get(edge.to.as_str())?,
                e,
            ))
        })
        .collect();
    let (loops, edges): (Vec<_>, Vec<_>) = edges.into_iter().partition(|(u, v, _)| u == v);

    // 1. Break cycles: edges back to a node on the DFS stack are reversed.
    let mut out: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (k, (u, v, _)) in edges.iter().enumerate() {
        out[*u].push((*v, k));
    }
    let mut reversed = vec![false; edges.len()];
    let mut state = vec![0u8; n];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        state[root] = 1;
        while let Some((u, i)) = stack.pop() {
            if let Some(&(v, k)) = out[u].get(i) {
                stack.push((u, i + 1));
                match state[v] {
                    0 => {
                        state[v] = 1;
                        stack.push((v, 0));
                    }
                    1 => reversed[k] = true,
                    _ => {}
                }
            } else {
                state[u] = 2;
            }
        }
    }
    let dag: Vec<(usize, usize)> = edges
        .iter()
        .enumerate()
        .map(|(k, (u, v, _))| if reversed[k] { (*v, *u) } else { (*u, *v) })
        .collect();

    // 2. Layers by longest path from the sources.
    let mut layer = vec![0usize; n];
    let mut indegree = vec![0usize; n];
    for (_, v) in &dag {
        indegree[*v] += 1;
    }
    let mut queue: Vec<usize> = (0..n).filter(|v| indegree[*v] == 0).collect();
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        for (a, b) in &dag {
            if *a == u {
                layer[*b] = layer[*b].max(layer[u] + 1);
                indegree[*b] -= 1;
                if indegree[*b] == 0 {
                    queue.push(*b);
                }
            }
        }
    }
    // Edge labels sit in their own layer between the nodes' layers.
    let labelled = edges.iter().any(|(_, _, e)| {
        g.edges[*e]
            .label
            .as_deref()
            .is_some_and(|l| !l.trim().is_empty())
    });
    if labelled {
        layer.iter_mut().for_each(|l| *l *= 2);
    }

    // 3. Waypoints for edges spanning more than one layer.
    let mut verts: Vec<Vertex> = sizes
        .iter()
        .map(|s| {
            let [b, l] = flow(*s);
            Vertex {
                breadth: b,
                length: l,
                real: true,
                label: false,
            }
        })
        .collect();
    let mut vlayer = layer.clone();
    let mut chains: Vec<Vec<usize>> = Vec::with_capacity(dag.len());
    let mut label_at: Vec<Option<EdgeLabel>> = Vec::with_capacity(dag.len());
    for (k, (u, v)) in dag.iter().enumerate() {
        let mut chain = vec![*u];
        let span = layer[*v] - layer[*u];
        let text = g.edges[edges[k].2]
            .label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty());
        let mid = span / 2;
        let mut label = None;
        for step in 1..span {
            let id = verts.len();
            let mut vertex = Vertex {
                breadth: 10.,
                length: 0.,
                real: false,
                label: false,
            };
            if step == mid
                && let Some(text) = text
            {
                let ls = lines(text);
                let [w, h] = block_size(&ls, EDGE_FONT, 15.);
                let s = [w + 12., h + 6.];
                let [b, l] = flow(s);
                vertex.breadth = b;
                vertex.length = l;
                vertex.label = true;
                label = Some((id, ls, s));
            }
            verts.push(vertex);
            vlayer.push(layer[*u] + step);
            chain.push(id);
        }
        chain.push(*v);
        chains.push(chain);
        label_at.push(label);
    }
    let layers_n = vlayer.iter().max().map_or(0, |m| m + 1);
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); verts.len()];
    let mut down: Vec<Vec<usize>> = vec![Vec::new(); verts.len()];
    for chain in &chains {
        for w in chain.windows(2) {
            down[w[0]].push(w[1]);
            up[w[1]].push(w[0]);
        }
    }

    // 4. Order within layers: declaration order, then barycentre sweeps,
    // keeping the order with the fewest crossings.
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); layers_n];
    for (v, l) in vlayer.iter().enumerate() {
        layers[*l].push(v);
    }
    let mut pos = vec![0f32; verts.len()];
    let renumber = |layers: &[Vec<usize>], pos: &mut Vec<f32>| {
        for l in layers {
            for (i, v) in l.iter().enumerate() {
                pos[*v] = i as f32;
            }
        }
    };
    renumber(&layers, &mut pos);
    let crossings = |layers: &[Vec<usize>], pos: &[f32]| {
        let mut count = 0;
        for l in layers.iter().take(layers.len().saturating_sub(1)) {
            let pairs: Vec<(f32, f32)> = l
                .iter()
                .flat_map(|u| down[*u].iter().map(move |v| (pos[*u], pos[*v])))
                .collect();
            for (i, a) in pairs.iter().enumerate() {
                for b in &pairs[i + 1..] {
                    if (a.0 - b.0) * (a.1 - b.1) < 0. {
                        count += 1;
                    }
                }
            }
        }
        count
    };
    let mut best = (crossings(&layers, &pos), layers.clone());
    for sweep in 0..24 {
        let downward = sweep % 2 == 0;
        let order: Vec<usize> = if downward {
            (1..layers_n).collect()
        } else {
            (0..layers_n.saturating_sub(1)).rev().collect()
        };
        for l in order {
            let mut keyed: Vec<(f32, usize)> = layers[l]
                .iter()
                .map(|v| {
                    let near = if downward { &up[*v] } else { &down[*v] };
                    let key = if near.is_empty() {
                        pos[*v]
                    } else {
                        near.iter().map(|u| pos[*u]).sum::<f32>() / near.len() as f32
                    };
                    (key, *v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[l] = keyed.into_iter().map(|(_, v)| v).collect();
            for (i, v) in layers[l].iter().enumerate() {
                pos[*v] = i as f32;
            }
        }
        let c = crossings(&layers, &pos);
        if c < best.0 {
            best = (c, layers.clone());
        }
        if best.0 == 0 {
            break;
        }
    }
    let layers = best.1;
    renumber(&layers, &mut pos);

    // 5. Positions across the layers: each vertex pulled toward its
    // neighbours, then the least-squares placement that keeps the gaps.
    let sep = |a: usize, b: usize| {
        let gap = if verts[a].real && verts[b].real {
            NODE_GAP
        } else if verts[a].label || verts[b].label {
            24.
        } else {
            14.
        };
        (verts[a].breadth + verts[b].breadth) / 2. + gap
    };
    let mut x = vec![0f32; verts.len()];
    for l in &layers {
        let mut at = 0.;
        for (i, v) in l.iter().enumerate() {
            if i > 0 {
                at += sep(l[i - 1], *v);
            }
            x[*v] = at;
        }
    }
    for pass in 0..10 {
        let order: Vec<usize> = match pass % 3 {
            0 => (1..layers_n).collect(),
            1 => (0..layers_n.saturating_sub(1)).rev().collect(),
            _ => (0..layers_n).collect(),
        };
        for l in order {
            let row = &layers[l];
            let desired: Vec<f32> = row
                .iter()
                .map(|v| {
                    let near: Vec<usize> = match pass % 3 {
                        0 => up[*v].clone(),
                        1 => down[*v].clone(),
                        _ => up[*v].iter().chain(&down[*v]).copied().collect(),
                    };
                    if near.is_empty() {
                        x[*v]
                    } else {
                        near.iter().map(|u| x[*u]).sum::<f32>() / near.len() as f32
                    }
                })
                .collect();
            let gaps: Vec<f32> = row.windows(2).map(|w| sep(w[0], w[1])).collect();
            for (v, p) in row.iter().zip(place(&desired, &gaps)) {
                x[*v] = p;
            }
        }
    }

    // 6. Positions along the flow.
    let mut layer_len = vec![0f32; layers_n];
    for (v, l) in vlayer.iter().enumerate() {
        layer_len[*l] = layer_len[*l].max(verts[v].length);
    }
    let gap = if labelled { HALF_GAP } else { LAYER_GAP };
    let mut centre = vec![0f32; layers_n];
    let mut at = 0.;
    for l in 0..layers_n {
        centre[l] = at + layer_len[l] / 2.;
        at += layer_len[l] + gap;
    }
    let main = |v: usize| centre[vlayer[v]];

    // Flow space (across, along) to the page; the far side for BT/RL is
    // mirrored once the extent is known.
    let span = at - gap;
    let to_page = |p: [f32; 2]| -> [f32; 2] {
        let along = match g.direction {
            Direction::BottomToTop | Direction::RightToLeft => span - p[1],
            _ => p[1],
        };
        if across { [along, p[0]] } else { [p[0], along] }
    };
    // Ports: where an edge leaves or enters a node along the flow. Edges
    // sharing a side spread across it, in the order of their other ends,
    // and stop at the shape's outline.
    let mut sides: HashMap<(usize, bool), Vec<(f32, usize)>> = HashMap::new();
    for (k, chain) in chains.iter().enumerate() {
        let (first, last) = (chain[0], chain[chain.len() - 1]);
        sides
            .entry((first, true))
            .or_default()
            .push((x[chain[1]], k));
        sides
            .entry((last, false))
            .or_default()
            .push((x[chain[chain.len() - 2]], k));
    }
    let mut offsets: HashMap<(usize, bool), f32> = HashMap::new();
    for ((v, leaving), mut list) in sides {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = list.len();
        if n < 2 {
            continue;
        }
        let room = verts[v].breadth
            * if g.nodes[v].shape == NodeShape::Diamond {
                0.35
            } else {
                0.6
            };
        let step = (room / (n - 1) as f32).min(16.);
        for (i, (_, k)) in list.into_iter().enumerate() {
            offsets.insert((k, leaving), (i as f32 - (n - 1) as f32 / 2.) * step);
        }
    }
    let port = |v: usize, leaving: bool, off: f32| -> [f32; 2] {
        let (half_b, half_l) = (verts[v].breadth / 2., verts[v].length / 2.);
        let t = (off / half_b.max(1.)).clamp(-1., 1.);
        let half = match g.nodes[v].shape {
            NodeShape::Diamond => half_l * (1. - t.abs()),
            NodeShape::Circle | NodeShape::DoubleCircle => half_l * (1. - t * t).sqrt(),
            _ => half_l,
        };
        to_page([x[v] + off, main(v) + if leaving { half } else { -half }])
    };

    let style_of = |id: &str| g.node_styles.get(id);
    let mut nodes: Vec<Shape> = (0..n)
        .map(|v| {
            let [cx, cy] = to_page([x[v], main(v)]);
            let [w, h] = sizes[v];
            let style = style_of(&g.nodes[v].id);
            Shape {
                rect: [cx - w / 2., cy - h / 2., w, h],
                kind: g.nodes[v].shape,
                lines: if marker(&g.nodes[v].shape, &g.nodes[v].label).is_some() {
                    Vec::new()
                } else {
                    labels[v].clone()
                },
                fill: style.and_then(|s| s.fill).map(hex),
                stroke: style.and_then(|s| s.stroke).map(hex),
                color: style.and_then(|s| s.color).map(hex),
            }
        })
        .collect();

    // 7. Edges as smooth curves through their waypoints, tangent to the
    // flow at every point; reversed edges are drawn back the right way.
    let mut wires = Vec::new();
    for (k, chain) in chains.iter().enumerate() {
        let edge = &g.edges[edges[k].2];
        let mut pts: Vec<[f32; 2]> = Vec::with_capacity(chain.len());
        pts.push(port(
            chain[0],
            true,
            offsets.get(&(k, true)).copied().unwrap_or(0.),
        ));
        for v in &chain[1..chain.len() - 1] {
            pts.push(to_page([x[*v], main(*v)]));
        }
        pts.push(port(
            chain[chain.len() - 1],
            false,
            offsets.get(&(k, false)).copied().unwrap_or(0.),
        ));
        if reversed[k] {
            pts.reverse();
        }
        let label = label_at[k].as_ref().map(|(v, ls, s)| {
            let [cx, cy] = to_page([x[*v], main(*v)]);
            (ls.clone(), [cx - s[0] / 2., cy - s[1] / 2., s[0], s[1]])
        });
        wires.push(Wire {
            start: pts[0],
            curves: smooth(&pts, across),
            style: edge.style,
            head: edge.end,
            tail: edge.start,
            label,
            color: g
                .edge_styles
                .get(&edges[k].2)
                .and_then(|s| s.stroke)
                .map(hex),
        });
    }
    for (u, _, e) in &loops {
        let [x0, y0, w, h] = nodes[*u].rect;
        let edge = &g.edges[*e];
        let (a, b) = ([x0 + w, y0 + h * 0.3], [x0 + w, y0 + h * 0.7]);
        let label = edge
            .label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|t| {
                let ls = lines(t);
                let [lw, lh] = block_size(&ls, EDGE_FONT, 15.);
                (
                    ls,
                    [
                        x0 + w + 34.,
                        y0 + h / 2. - (lh + 6.) / 2.,
                        lw + 12.,
                        lh + 6.,
                    ],
                )
            });
        wires.push(Wire {
            start: a,
            curves: vec![[[a[0] + 34., a[1] - 14.], [b[0] + 34., b[1] + 14.], b]],
            style: edge.style,
            head: edge.end,
            tail: edge.start,
            label,
            color: None,
        });
    }

    // 8. Subgraphs: boxes around their members, inner ones first.
    let mut clusters = Vec::new();
    let mut boxes: HashMap<&str, [f32; 4]> = HashMap::new();
    fn bounds(a: Option<[f32; 4]>, r: [f32; 4]) -> Option<[f32; 4]> {
        Some(match a {
            None => r,
            Some(b) => {
                let (x0, y0) = (b[0].min(r[0]), b[1].min(r[1]));
                let (x1, y1) = (
                    (b[0] + b[2]).max(r[0] + r[2]),
                    (b[1] + b[3]).max(r[1] + r[3]),
                );
                [x0, y0, x1 - x0, y1 - y0]
            }
        })
    }
    let mut pending: Vec<&mermaid_text::types::Subgraph> = g.subgraphs.iter().collect();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|s| {
            if !s
                .subgraph_ids
                .iter()
                .all(|c| boxes.contains_key(c.as_str()))
            {
                return true;
            }
            let mut r = None;
            for id in &s.node_ids {
                if let Some(v) = index.get(id.as_str()) {
                    r = bounds(r, nodes[*v].rect);
                }
            }
            for c in &s.subgraph_ids {
                r = bounds(r, boxes[c.as_str()]);
            }
            if let Some([x0, y0, w, h]) = r {
                let rect = [x0 - 14., y0 - 30., w + 28., h + 44.];
                boxes.insert(s.id.as_str(), rect);
                clusters.push(Cluster {
                    rect,
                    label: s.label.clone(),
                });
            } else {
                boxes.insert(s.id.as_str(), [0.; 4]);
            }
            false
        });
        if pending.len() == before {
            break;
        }
    }
    clusters.reverse();

    // 9. Move everything onto the page.
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    let mut grow = |r: [f32; 4]| {
        lo = [lo[0].min(r[0]), lo[1].min(r[1])];
        hi = [hi[0].max(r[0] + r[2]), hi[1].max(r[1] + r[3])];
    };
    nodes.iter().for_each(|s| grow(s.rect));
    clusters.iter().for_each(|c| grow(c.rect));
    for w in &wires {
        grow([w.start[0], w.start[1], 0., 0.]);
        for c in &w.curves {
            for p in c {
                grow([p[0], p[1], 0., 0.]);
            }
        }
        if let Some((_, r)) = &w.label {
            grow(*r);
        }
    }
    if nodes.is_empty() {
        lo = [0.; 2];
        hi = [0.; 2];
    }
    let (dx, dy) = (MARGIN - lo[0], MARGIN - lo[1]);
    let shift = |p: &mut [f32; 2]| {
        p[0] += dx;
        p[1] += dy;
    };
    for s in &mut nodes {
        s.rect[0] += dx;
        s.rect[1] += dy;
    }
    for c in &mut clusters {
        c.rect[0] += dx;
        c.rect[1] += dy;
    }
    for w in &mut wires {
        shift(&mut w.start);
        w.curves.iter_mut().flatten().for_each(&shift);
        if let Some((_, r)) = &mut w.label {
            r[0] += dx;
            r[1] += dy;
        }
    }
    Graph {
        size: [hi[0] - lo[0] + 2. * MARGIN, hi[1] - lo[1] + 2. * MARGIN],
        nodes,
        edges: wires,
        clusters,
    }
}

/// Positions as close as possible to `desired` (least squares) while
/// keeping at least `gaps[i]` between neighbours i and i+1.
fn place(desired: &[f32], gaps: &[f32]) -> Vec<f32> {
    let mut offset = vec![0f32; desired.len()];
    for i in 1..desired.len() {
        offset[i] = offset[i - 1] + gaps[i - 1];
    }
    // Isotonic regression of desired - offset (pool adjacent violators).
    let mut blocks: Vec<(f32, usize)> = Vec::new();
    for (d, o) in desired.iter().zip(&offset) {
        blocks.push((d - o, 1));
        while blocks.len() > 1 {
            let (s2, c2) = blocks[blocks.len() - 1];
            let (s1, c1) = blocks[blocks.len() - 2];
            if s1 / c1 as f32 > s2 / c2 as f32 {
                blocks.pop();
                *blocks.last_mut().unwrap() = (s1 + s2, c1 + c2);
            } else {
                break;
            }
        }
    }
    let mut out = Vec::with_capacity(desired.len());
    for (s, c) in blocks {
        for _ in 0..c {
            out.push(s / c as f32);
        }
    }
    out.iter_mut().zip(&offset).for_each(|(p, o)| *p += o);
    out
}

/// Cubic segments through `pts`, leaving and entering every point along
/// the flow, like Mermaid's curves.
fn smooth(pts: &[[f32; 2]], across: bool) -> Vec<[[f32; 2]; 3]> {
    pts.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            if across {
                let mid = (b[0] - a[0]) / 2.;
                [[a[0] + mid, a[1]], [b[0] - mid, b[1]], b]
            } else {
                let mid = (b[1] - a[1]) / 2.;
                [[a[0], a[1] + mid], [b[0], b[1] - mid], b]
            }
        })
        .collect()
}

// ── Sequence diagrams ───────────────────────────────────────────────────

pub struct Sequence {
    pub size: [f32; 2],
    actors: Vec<(f32, [f32; 2], Vec<String>)>,
    /// Lifelines run between the header and footer boxes.
    life: [f32; 2],
    messages: Vec<Arrow>,
    notes: Vec<([f32; 4], Vec<String>)>,
    bars: Vec<[f32; 4]>,
    frames: Vec<Frame>,
}

struct Arrow {
    from: f32,
    to: f32,
    y: f32,
    dashed: bool,
    head: bool,
    lines: Vec<String>,
    /// Self-messages loop out to the right and back.
    looped: bool,
}

struct Frame {
    rect: [f32; 4],
    tag: String,
    label: String,
    /// Branch separators (y, label).
    splits: Vec<(f32, String)>,
}

const ACTOR_H: f32 = 38.;

fn sequence(s: &SequenceDiagram) -> Sequence {
    let index: HashMap<&str, usize> = s
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    let labels: Vec<Vec<String>> = s.participants.iter().map(|p| lines(&p.label)).collect();
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| (block_size(l, FONT, LINE)[0] + 2. * PAD_X).max(84.))
        .collect();
    let heights: Vec<f32> = labels
        .iter()
        .map(|l| (l.len() as f32 * LINE + 2. * PAD_Y).max(ACTOR_H))
        .collect();
    let n = widths.len();
    // Autonumbering, applied in step with the messages.
    let mut numbers = vec![None; s.messages.len()];
    let mut next: Option<u32> = None;
    let mut changes = s.autonumber_changes.iter().peekable();
    for (m, slot) in numbers.iter_mut().enumerate() {
        while let Some(c) = changes.peek().filter(|c| c.at_message <= m) {
            next = match c.state {
                mermaid_text::sequence::AutonumberState::On { next_value } => Some(next_value),
                mermaid_text::sequence::AutonumberState::Off => None,
            };
            changes.next();
        }
        if let Some(v) = next.as_mut() {
            *slot = Some(*v);
            *v += 1;
        }
    }
    let message_lines: Vec<Vec<String>> = s
        .messages
        .iter()
        .zip(&numbers)
        .map(|(m, num)| {
            let mut ls = lines(&m.text);
            if let (Some(num), Some(first)) = (num, ls.first_mut()) {
                *first = format!("{num}. {first}");
            }
            ls
        })
        .collect();

    // Column gaps wide enough for the boxes and the messages between them.
    let mut gap: Vec<f32> = (1..n)
        .map(|i| (widths[i - 1] + widths[i]) / 2. + 36.)
        .collect();
    let mut tail = 0f32;
    for (m, msg) in s.messages.iter().enumerate() {
        let (Some(&a), Some(&b)) = (index.get(msg.from.as_str()), index.get(msg.to.as_str()))
        else {
            continue;
        };
        let need = block_size(&message_lines[m], EDGE_FONT, 15.)[0] + 28.;
        if a == b {
            if a + 1 < n {
                gap[a] = gap[a].max(need + 46.);
            } else {
                tail = tail.max(need + 46.);
            }
            continue;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        let have: f32 = gap[lo..hi].iter().sum();
        if have < need {
            gap[hi - 1] += need - have;
        }
    }
    for note in &s.notes {
        let w = block_size(&lines(&note.text), EDGE_FONT, 15.)[0] + 20.;
        if let NoteAnchor::OverPair(a, b) = &note.anchor
            && let (Some(&a), Some(&b)) = (index.get(a.as_str()), index.get(b.as_str()))
        {
            let (lo, hi) = (a.min(b), a.max(b));
            let have: f32 = gap[lo..hi].iter().sum();
            if hi > lo && have < w {
                gap[hi - 1] += w - have;
            }
        }
    }
    let mut cx = vec![widths.first().map_or(0., |w| w / 2.)];
    for g in &gap {
        cx.push(cx.last().unwrap() + g);
    }

    // Rows, top to bottom.
    let head = heights.iter().copied().fold(ACTOR_H, f32::max);
    let mut y = head + 22.;
    let mut messages = Vec::new();
    let mut notes = Vec::new();
    let mut frame_top: HashMap<usize, f32> = HashMap::new();
    let mut frame_bottom: HashMap<usize, f32> = HashMap::new();
    let mut splits: HashMap<usize, Vec<(f32, String)>> = HashMap::new();
    let mut message_y = vec![0f32; s.messages.len()];
    let place_notes = |at: usize, y: &mut f32, notes: &mut Vec<([f32; 4], Vec<String>)>| {
        for note in s.notes.iter().filter(|n| n.after_message == at) {
            let ls = lines(&note.text);
            let [w, h] = block_size(&ls, EDGE_FONT, 15.);
            let (w, h) = (w + 20., h + 12.);
            let col = |id: &str| index.get(id).map(|i| cx[*i]);
            let placed = match &note.anchor {
                NoteAnchor::Over(p) => col(p).map(|c| (c - w / 2., w)),
                NoteAnchor::LeftOf(p) => col(p).map(|c| (c - 14. - w, w)),
                NoteAnchor::RightOf(p) => col(p).map(|c| (c + 14., w)),
                NoteAnchor::OverPair(a, b) => match (col(a), col(b)) {
                    (Some(a), Some(b)) => {
                        let w = w.max((a - b).abs() + 40.);
                        Some(((a + b) / 2. - w / 2., w))
                    }
                    _ => None,
                },
            };
            if let Some((x, w)) = placed {
                notes.push(([x, *y, w, h], ls));
                *y += h + 12.;
            }
        }
    };
    for (m, msg) in s.messages.iter().enumerate() {
        // Notes written before a block opens sit above its frame.
        place_notes(m, &mut y, &mut notes);
        let mut starting: Vec<(usize, usize)> = s
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.start_message == m && b.end_message >= b.start_message)
            .map(|(i, b)| (i, b.end_message))
            .collect();
        // Outer blocks (ending later) first.
        starting.sort_by_key(|s| std::cmp::Reverse(s.1));
        for (i, _) in starting {
            frame_top.insert(i, y);
            y += 28.;
        }
        for (i, b) in s.blocks.iter().enumerate() {
            for br in b.branches.iter().skip(1).filter(|br| br.start_message == m) {
                splits.entry(i).or_default().push((y, br.label.clone()));
                y += 26.;
            }
        }
        let (Some(&a), Some(&b)) = (index.get(msg.from.as_str()), index.get(msg.to.as_str()))
        else {
            continue;
        };
        let label_h = message_lines[m].len() as f32 * 15.;
        y += label_h + 4.;
        message_y[m] = y;
        messages.push(Arrow {
            from: cx[a],
            to: cx[b],
            y,
            dashed: msg.style.is_dashed(),
            head: msg.style.has_arrow(),
            lines: message_lines[m].clone(),
            looped: a == b,
        });
        y += if a == b { 40. } else { 20. };
        for (i, b) in s.blocks.iter().enumerate() {
            if b.end_message == m && frame_top.contains_key(&i) {
                frame_bottom.insert(i, y);
                y += 12.;
            }
        }
    }
    place_notes(s.messages.len(), &mut y, &mut notes);
    let foot = y + 10.;

    let bars: Vec<[f32; 4]> = s
        .activations
        .iter()
        .filter_map(|a| {
            let c = cx[*index.get(a.participant.as_str())?];
            let top = message_y.get(a.start_message).copied()? - 4.;
            let bottom = message_y.get(a.end_message).copied().unwrap_or(foot - 10.) + 4.;
            Some([c - 5., top, 10., (bottom - top).max(12.)])
        })
        .collect();

    // Frames span the lanes their messages touch.
    let mut frames = Vec::new();
    for (i, b) in s.blocks.iter().enumerate() {
        let (Some(top), Some(bottom)) = (frame_top.get(&i), frame_bottom.get(&i)) else {
            continue;
        };
        let lanes: Vec<usize> = s.messages
            [b.start_message..=b.end_message.min(s.messages.len() - 1)]
            .iter()
            .flat_map(|m| {
                [
                    index.get(m.from.as_str()).copied(),
                    index.get(m.to.as_str()).copied(),
                ]
            })
            .flatten()
            .collect();
        let (lo, hi) = match (lanes.iter().min(), lanes.iter().max()) {
            (Some(lo), Some(hi)) => (cx[*lo], cx[*hi]),
            _ => continue,
        };
        let depth = s
            .blocks
            .iter()
            .filter(|o| {
                o.start_message <= b.start_message
                    && o.end_message >= b.end_message
                    && !std::ptr::eq(*o, b)
            })
            .count() as f32;
        let pad = 56. - depth * 8.;
        let tag = format!("{:?}", b.kind).to_lowercase();
        frames.push(Frame {
            rect: [
                lo - pad,
                top - 4.,
                hi - lo + 2. * pad + if hi == lo { 60. } else { 0. },
                bottom - top + 4.,
            ],
            tag,
            label: b
                .branches
                .first()
                .map(|br| br.label.clone())
                .unwrap_or_default(),
            splits: splits.remove(&i).unwrap_or_default(),
        });
    }

    let actors: Vec<(f32, [f32; 2], Vec<String>)> = (0..n)
        .map(|i| (cx[i], [widths[i], heights[i]], labels[i].clone()))
        .collect();
    // Extent, including notes and frames that reach past the lanes.
    let mut lo = 0f32;
    let mut hi = cx.last().copied().unwrap_or(0.) + widths.last().map_or(0., |w| w / 2.) + tail;
    for (r, _) in &notes {
        lo = lo.min(r[0]);
        hi = hi.max(r[0] + r[2]);
    }
    for f in &frames {
        lo = lo.min(f.rect[0]);
        hi = hi.max(f.rect[0] + f.rect[2]);
    }
    for m in &messages {
        if m.looped {
            hi = hi.max(
                m.from + 40. + text_width(m.lines.first().map_or("", String::as_str), EDGE_FONT),
            );
        }
    }
    let dx = MARGIN - lo;
    let mut out = Sequence {
        size: [hi - lo + 2. * MARGIN, foot + head + 2. * MARGIN],
        actors,
        life: [head + MARGIN, foot + MARGIN],
        messages,
        notes,
        bars,
        frames,
    };
    for a in &mut out.actors {
        a.0 += dx;
    }
    for m in &mut out.messages {
        m.from += dx;
        m.to += dx;
        m.y += MARGIN;
    }
    for (r, _) in &mut out.notes {
        r[0] += dx;
        r[1] += MARGIN;
    }
    for r in &mut out.bars {
        r[0] += dx;
        r[1] += MARGIN;
    }
    for f in &mut out.frames {
        f.rect[0] += dx;
        f.rect[1] += MARGIN;
        f.splits.iter_mut().for_each(|s| s.0 += MARGIN);
    }
    out
}

// ── Drawing ─────────────────────────────────────────────────────────────

fn rounded(r: [f32; 4], radius: f32) -> VectorPath {
    let [x, y, w, h] = r;
    let k = radius.min(w / 2.).min(h / 2.);
    VectorPath::new()
        .move_to(x + k, y)
        .line_to(x + w - k, y)
        .quad_to([x + w, y], [x + w, y + k])
        .line_to(x + w, y + h - k)
        .quad_to([x + w, y + h], [x + w - k, y + h])
        .line_to(x + k, y + h)
        .quad_to([x, y + h], [x, y + h - k])
        .line_to(x, y + k)
        .quad_to([x, y], [x + k, y])
        .close()
}

fn polygon(points: &[[f32; 2]]) -> VectorPath {
    let mut p = VectorPath::new().move_to(points[0][0], points[0][1]);
    for q in &points[1..] {
        p = p.line_to(q[0], q[1]);
    }
    p.close()
}

/// The outline of a node shape.
fn outline(kind: &NodeShape, r: [f32; 4]) -> VectorPath {
    let [x, y, w, h] = r;
    let (cx, cy) = (x + w / 2., y + h / 2.);
    match kind {
        NodeShape::Rounded => rounded(r, 10.),
        NodeShape::Stadium => rounded(r, h / 2.),
        NodeShape::Diamond => polygon(&[[cx, y], [x + w, cy], [cx, y + h], [x, cy]]),
        NodeShape::Circle | NodeShape::DoubleCircle => VectorPath::ellipse(cx, cy, w / 2., h / 2.),
        NodeShape::Hexagon => {
            let k = h * 0.3;
            polygon(&[
                [x + k, y],
                [x + w - k, y],
                [x + w, cy],
                [x + w - k, y + h],
                [x + k, y + h],
                [x, cy],
            ])
        }
        NodeShape::Parallelogram => {
            polygon(&[[x + 14., y], [x + w, y], [x + w - 14., y + h], [x, y + h]])
        }
        NodeShape::Trapezoid => {
            polygon(&[[x + 14., y], [x + w - 14., y], [x + w, y + h], [x, y + h]])
        }
        NodeShape::Asymmetric => polygon(&[
            [x, y],
            [x + w, y],
            [x + w, y + h],
            [x, y + h],
            [x + 14., cy],
        ]),
        NodeShape::Bar(_) => rounded(r, 2.),
        _ => rounded(r, 5.),
    }
}

/// Points along a wire, for dashes and arrowheads.
fn flatten(w: &Wire) -> Vec<[f32; 2]> {
    let mut out = vec![w.start];
    let mut a = w.start;
    for [c1, c2, b] in &w.curves {
        for i in 1..=16 {
            let t = i as f32 / 16.;
            let u = 1. - t;
            let p = |k: usize| {
                u * u * u * a[k]
                    + 3. * u * u * t * c1[k]
                    + 3. * u * t * t * c2[k]
                    + t * t * t * b[k]
            };
            out.push([p(0), p(1)]);
        }
        a = *b;
    }
    out
}

fn dashes(points: &[[f32; 2]], on: f32, off: f32) -> VectorPath {
    let mut path = VectorPath::new();
    let mut drawing = true;
    let mut left = on;
    for w in points.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        if drawing {
            path = path.move_to(a[0], a[1]);
        }
        while len > 0. {
            let step = left.min(len);
            let t = step / len;
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if drawing {
                path = path.line_to(p[0], p[1]);
            }
            len -= step;
            left -= step;
            a = p;
            if left <= 0. {
                drawing = !drawing;
                left = if drawing { on } else { off };
                if drawing {
                    path = path.move_to(a[0], a[1]);
                }
            }
        }
    }
    path
}

fn arrowhead(tip: [f32; 2], from: [f32; 2], size: f32) -> VectorPath {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / len, dy / len);
    let base = [tip[0] - ux * size, tip[1] - uy * size];
    let half = size * 0.45;
    polygon(&[
        tip,
        [base[0] - uy * half, base[1] + ux * half],
        [base[0] + uy * half, base[1] - ux * half],
    ])
}

fn endpoint(
    c: &mut Canvas,
    kind: EdgeEndpoint,
    tip: [f32; 2],
    from: [f32; 2],
    color: &str,
    s: f32,
) {
    match kind {
        EdgeEndpoint::Arrow => c.fill(arrowhead(tip, from, 9. * s), color),
        EdgeEndpoint::Circle => {
            c.fill(
                VectorPath::circle(tip[0], tip[1], 4. * s),
                ColorToken::Base200,
            );
            c.stroke(VectorPath::circle(tip[0], tip[1], 4. * s), color, 1.5 * s);
        }
        EdgeEndpoint::Cross => {
            let k = 4. * s;
            c.stroke(
                VectorPath::new()
                    .move_to(tip[0] - k, tip[1] - k)
                    .line_to(tip[0] + k, tip[1] + k)
                    .move_to(tip[0] - k, tip[1] + k)
                    .line_to(tip[0] + k, tip[1] - k),
                color,
                1.6 * s,
            );
        }
        EdgeEndpoint::None => {}
    }
}

fn scaled(r: [f32; 4], s: f32) -> [f32; 4] {
    [r[0] * s, r[1] * s, r[2] * s, r[3] * s]
}

/// A label centred in `rect`, placed by padding so layout, hit testing
/// and selection all agree on where it is.
fn label_at<C: crate::Host>(
    lines: &[String],
    rect: [f32; 4],
    size: f32,
    line: f32,
    color: Option<&str>,
) -> Element<C> {
    let size = size.max(8.);
    let mut t = crate::literal(&lines.join("\n"))
        .font_size(size)
        .line_height(line / size)
        .text_align(TextAlign::Center)
        .no_wrap(true);
    if let Some(color) = color {
        t = t.color(color.to_owned());
    }
    vstack![
        vstack![t]
            .width(rect[2])
            .height(rect[3])
            .align(Align::Center)
            .justify(Justify::Center)
    ]
    .padding(rect[0], Edge::Left)
    .padding(rect[1], Edge::Top)
    .into()
}

/// The diagram drawn at scale `s`; text diagrams use `mono`.
pub fn view<C: crate::Host>(d: &Diagram, s: f32, mono: &str, mono_size: f32) -> Element<C> {
    match d {
        Diagram::Pending => text("Rendering diagram…").into(),
        Diagram::Graph(g) => graph_view(g, s),
        Diagram::Sequence(q) => sequence_view(q, s),
        Diagram::Pie { title, slices } => {
            let mut parts: Vec<Element<C>> = Vec::new();
            if let Some(title) = title {
                parts.push(crate::literal(title).font_size(14. * s).font_weight(600));
            }
            parts.push(
                donut_chart(slices.clone())
                    .size(180. * s)
                    .thickness(30. * s)
                    .into(),
            );
            vstack(parts)
                .gap(10., Axis::Vertical)
                .align(Align::Center)
                .into()
        }
        Diagram::Image { image, size } => vstack![]
            .width(size[0] * s)
            .height(size[1] * s)
            .background_image(image.clone().background().fit(ImageFit::Contain))
            .accessible_label("Mermaid diagram")
            .into(),
        Diagram::Text(art) => crate::literal(art)
            .font_family(mono.to_owned())
            .font_size(mono_size)
            .line_height(1.2)
            .no_wrap(true),
        Diagram::Error(e) => crate::literal(e),
    }
}

/// Split retained strokes at path/element budgets instead of letting the kit
/// silently omit later geometry. Shape fills here are small closed primitives.
fn canvas_layers<C: 'static>(scene: Canvas, size: [f32; 2]) -> Vec<Element<C>> {
    use volt_ui_kit::graphics::{DrawPath, MAX_CANVAS_PATHS, MAX_PATH_COMMANDS, PathCommand};
    let mut paths = Vec::new();
    for draw in scene.paths {
        if draw.stroke.is_none() || draw.path.commands.len() <= MAX_PATH_COMMANDS {
            paths.push(draw);
            continue;
        }
        let mut part = Vec::new();
        let (mut at, mut origin) = ([0.; 2], [0.; 2]);
        for command in draw.path.commands {
            if part.len() >= MAX_PATH_COMMANDS - 1 {
                paths.push(DrawPath {
                    path: VectorPath {
                        commands: std::mem::take(&mut part),
                    },
                    color: draw.color.clone(),
                    stroke: draw.stroke,
                });
                part.push(PathCommand::Move(at));
            }
            match &command {
                PathCommand::Move(p) => {
                    at = *p;
                    origin = *p;
                }
                PathCommand::Line(p) => at = *p,
                PathCommand::Quad(_, p) | PathCommand::Cubic(_, _, p) => at = *p,
                PathCommand::Close => {
                    part.push(PathCommand::Line(origin));
                    at = origin;
                    continue;
                }
            }
            part.push(command);
        }
        if !part.is_empty() {
            paths.push(DrawPath {
                path: VectorPath { commands: part },
                color: draw.color,
                stroke: draw.stroke,
            });
        }
    }
    paths
        .chunks(MAX_CANVAS_PATHS)
        .map(|paths| {
            let paths = paths.to_vec();
            canvas(move |c| c.paths = paths)
                .width(size[0])
                .height(size[1])
                .into()
        })
        .collect()
}

fn graph_view<C: crate::Host>(g: &Graph, s: f32) -> Element<C> {
    let edge_color: String = ColorToken::Muted.into();
    let muted = Some(edge_color.as_str());
    // The canvas closure runs while the view is built, so it borrows `g`.
    let mut scene = Canvas::default();
    {
        let c = &mut scene;
        {
            for cluster in &g.clusters {
                let r = scaled(cluster.rect, s);
                c.fill(rounded(r, 8. * s), ColorToken::Muted.alpha(8));
                c.stroke(rounded(r, 8. * s), ColorToken::Border, 1.);
            }
            for w in &g.edges {
                let mut path = VectorPath::new().move_to(w.start[0] * s, w.start[1] * s);
                for [a, b, e] in &w.curves {
                    path = path.cubic_to(
                        [a[0] * s, a[1] * s],
                        [b[0] * s, b[1] * s],
                        [e[0] * s, e[1] * s],
                    );
                }
                let pts: Vec<[f32; 2]> = flatten(w)
                    .into_iter()
                    .map(|p| [p[0] * s, p[1] * s])
                    .collect();
                let color = w.color.clone().unwrap_or_else(|| edge_color.clone());
                let (style, head, tail) = (&w.style, &w.head, &w.tail);
                let width = match style {
                    EdgeStyle::Thick => 3. * s,
                    _ => 1.5 * s,
                };
                if matches!(style, EdgeStyle::Dotted) {
                    c.stroke(dashes(&pts, 4. * s, 4. * s), color.clone(), width);
                } else {
                    c.stroke(path, color.clone(), width);
                }
                let n = pts.len();
                if n >= 2 {
                    endpoint(c, *head, pts[n - 1], pts[n.saturating_sub(4)], &color, s);
                    endpoint(c, *tail, pts[0], pts[3.min(n - 1)], &color, s);
                }
            }
            for (_, r) in g.edges.iter().filter_map(|w| w.label.as_ref()) {
                c.fill(rounded(scaled(*r, s), 4. * s), ColorToken::Base200);
            }
            for node in &g.nodes {
                let (r, kind) = (scaled(node.rect, s), &node.kind);
                let path = outline(kind, r);
                if node.lines.is_empty() {
                    // State diagram start (filled dot) and end (ringed dot).
                    let (cx, cy, rad) = (r[0] + r[2] / 2., r[1] + r[3] / 2., r[2] / 2.);
                    if matches!(kind, NodeShape::DoubleCircle) {
                        c.stroke(
                            VectorPath::circle(cx, cy, rad - 0.75),
                            ColorToken::BaseContent,
                            1.5,
                        );
                        c.fill(
                            VectorPath::circle(cx, cy, rad * 0.55),
                            ColorToken::BaseContent,
                        );
                    } else {
                        c.fill(VectorPath::circle(cx, cy, rad), ColorToken::BaseContent);
                    }
                    continue;
                }
                let note = matches!(kind, NodeShape::Note);
                let fill = node.fill.clone().unwrap_or_else(|| {
                    if note {
                        ColorToken::Warning.alpha(16)
                    } else {
                        ColorToken::Primary.alpha(12)
                    }
                });
                let stroke = node.stroke.clone().unwrap_or_else(|| {
                    if note {
                        ColorToken::Warning.alpha(60)
                    } else {
                        ColorToken::Primary.alpha(70)
                    }
                });
                c.fill(path.clone(), ColorToken::Base100);
                c.fill(path.clone(), fill);
                c.stroke(path, stroke.clone(), 1.3 * s);
                match kind {
                    NodeShape::DoubleCircle => {
                        let (cx, cy) = (r[0] + r[2] / 2., r[1] + r[3] / 2.);
                        c.stroke(
                            VectorPath::ellipse(cx, cy, r[2] / 2. - 4. * s, r[3] / 2. - 4. * s),
                            stroke,
                            1.2 * s,
                        );
                    }
                    NodeShape::Subroutine => {
                        let k = 7. * s;
                        c.stroke(
                            VectorPath::new()
                                .move_to(r[0] + k, r[1])
                                .line_to(r[0] + k, r[1] + r[3])
                                .move_to(r[0] + r[2] - k, r[1])
                                .line_to(r[0] + r[2] - k, r[1] + r[3]),
                            stroke,
                            1.2 * s,
                        );
                    }
                    NodeShape::Cylinder => {
                        let k = 7. * s;
                        let (x, y, w) = (r[0], r[1], r[2]);
                        c.stroke(
                            VectorPath::new().move_to(x, y + k).cubic_to(
                                [x, y + 2. * k],
                                [x + w, y + 2. * k],
                                [x + w, y + k],
                            ),
                            stroke,
                            1.2 * s,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    let mut layers = canvas_layers(scene, [g.size[0] * s, g.size[1] * s]);
    for c in &g.clusters {
        let r = scaled(c.rect, s);
        layers.push(label_at(
            std::slice::from_ref(&c.label),
            [
                r[0] + 10. * s,
                r[1] + 6. * s,
                text_width(&c.label, 12.) * s + 4.,
                16. * s,
            ],
            12. * s,
            16. * s,
            muted,
        ));
    }
    for w in &g.edges {
        if let Some((ls, r)) = &w.label {
            layers.push(label_at(ls, scaled(*r, s), EDGE_FONT * s, 15. * s, muted));
        }
    }
    for n in &g.nodes {
        if n.lines.is_empty() {
            continue;
        }
        layers.push(label_at(
            &n.lines,
            scaled(n.rect, s),
            FONT * s,
            LINE * s,
            n.color.as_deref(),
        ));
    }
    zstack(layers)
        .width(g.size[0] * s)
        .height(g.size[1] * s)
        .flow(Flow::TopLeft)
        .into()
}

fn sequence_view<C: crate::Host>(q: &Sequence, s: f32) -> Element<C> {
    let line_color: String = ColorToken::Muted.into();
    let muted_text = line_color.clone();
    let muted = Some(muted_text.as_str());
    let actors: Vec<[f32; 4]> = q
        .actors
        .iter()
        .flat_map(|(cx, [w, h], _)| {
            [
                [(cx - w / 2.) * s, MARGIN * s, w * s, h * s],
                [(cx - w / 2.) * s, q.life[1] * s, w * s, h * s],
            ]
        })
        .collect();
    let lifelines: Vec<f32> = q.actors.iter().map(|a| a.0 * s).collect();
    let life = [q.life[0] * s, q.life[1] * s];
    let arrows: Vec<(f32, f32, f32, bool, bool, bool)> = q
        .messages
        .iter()
        .map(|m| (m.from * s, m.to * s, m.y * s, m.dashed, m.head, m.looped))
        .collect();
    let notes: Vec<[f32; 4]> = q.notes.iter().map(|(r, _)| scaled(*r, s)).collect();
    let bars: Vec<[f32; 4]> = q.bars.iter().map(|r| scaled(*r, s)).collect();
    let frames: Vec<([f32; 4], f32, Vec<f32>)> = q
        .frames
        .iter()
        .map(|f| {
            (
                scaled(f.rect, s),
                (text_width(&f.tag, 11.) + 16.) * s,
                f.splits.iter().map(|(y, _)| y * s).collect(),
            )
        })
        .collect();
    let mut scene = Canvas::default();
    {
        let c = &mut scene;
        for (r, tag_w, splits) in &frames {
            c.stroke(rounded(*r, 4. * s), ColorToken::Muted.alpha(60), 1.);
            let tag = [r[0], r[1], *tag_w, 18. * s];
            c.fill(
                polygon(&[
                    [tag[0], tag[1]],
                    [tag[0] + tag[2], tag[1]],
                    [tag[0] + tag[2], tag[1] + tag[3] - 5. * s],
                    [tag[0] + tag[2] - 6. * s, tag[1] + tag[3]],
                    [tag[0], tag[1] + tag[3]],
                ]),
                ColorToken::Muted.alpha(16),
            );
            for y in splits {
                c.stroke(
                    dashes(&[[r[0], *y], [r[0] + r[2], *y]], 5. * s, 4. * s),
                    ColorToken::Muted.alpha(60),
                    1.,
                );
            }
        }
        for x in &lifelines {
            c.stroke(
                dashes(&[[*x, life[0]], [*x, life[1]]], 5. * s, 4. * s),
                ColorToken::Border,
                1.2,
            );
        }
        for r in &bars {
            c.fill(rounded(*r, 2.), ColorToken::Base100);
            c.fill(rounded(*r, 2.), ColorToken::Primary.alpha(20));
            c.stroke(rounded(*r, 2.), ColorToken::Primary.alpha(70), 1.);
        }
        for r in &actors {
            c.fill(rounded(*r, 6. * s), ColorToken::Base100);
            c.fill(rounded(*r, 6. * s), ColorToken::Primary.alpha(12));
            c.stroke(rounded(*r, 6. * s), ColorToken::Primary.alpha(70), 1.3 * s);
        }
        for r in &notes {
            c.fill(rounded(*r, 3. * s), ColorToken::Base100);
            c.fill(rounded(*r, 3. * s), ColorToken::Warning.alpha(16));
            c.stroke(rounded(*r, 3. * s), ColorToken::Warning.alpha(60), 1.);
        }
        for (from, to, y, dashed, head, looped) in &arrows {
            let pts: Vec<[f32; 2]> = if *looped {
                let k = 34. * s;
                vec![
                    [*from, *y],
                    [from + k, *y],
                    [from + k, y + 18. * s],
                    [*from + 6. * s, y + 18. * s],
                ]
            } else {
                let dir = (to - from).signum();
                vec![[*from + dir * 5. * s, *y], [to - dir * 6. * s, *y]]
            };
            let path = if *dashed {
                dashes(&pts, 5. * s, 4. * s)
            } else {
                let mut p = VectorPath::new().move_to(pts[0][0], pts[0][1]);
                for q in &pts[1..] {
                    p = p.line_to(q[0], q[1]);
                }
                p
            };
            c.stroke(path, line_color.clone(), 1.4 * s);
            if *head {
                let n = pts.len();
                c.fill(
                    arrowhead(pts[n - 1], pts[n - 2], 9. * s),
                    line_color.clone(),
                );
            }
        }
    }
    let mut layers = canvas_layers(scene, [q.size[0] * s, q.size[1] * s]);
    for (cx, [w, h], ls) in &q.actors {
        for top in [MARGIN, q.life[1]] {
            layers.push(label_at(
                ls,
                scaled([cx - w / 2., top, *w, *h], s),
                FONT * s,
                LINE * s,
                None,
            ));
        }
    }
    for m in &q.messages {
        let h = m.lines.len() as f32 * 15.;
        let rect = if m.looped {
            let w = block_size(&m.lines, EDGE_FONT, 15.)[0] + 8.;
            [m.from + 40., m.y - 2., w, h]
        } else {
            let (lo, hi) = (m.from.min(m.to), m.from.max(m.to));
            [lo, m.y - h - 3., hi - lo, h]
        };
        layers.push(label_at(
            &m.lines,
            scaled(rect, s),
            EDGE_FONT * s,
            15. * s,
            None,
        ));
    }
    for (r, ls) in &q.notes {
        layers.push(label_at(ls, scaled(*r, s), EDGE_FONT * s, 15. * s, None));
    }
    for f in &q.frames {
        let tag_w = text_width(&f.tag, 11.) + 16.;
        layers.push(label_at(
            std::slice::from_ref(&f.tag),
            scaled([f.rect[0], f.rect[1], tag_w, 18.], s),
            11. * s,
            14. * s,
            muted,
        ));
        if !f.label.is_empty() {
            let text = format!("[{}]", f.label);
            let w = text_width(&text, 11.5) + 8.;
            layers.push(label_at(
                &[text],
                scaled([f.rect[0] + tag_w + 4., f.rect[1], w, 18.], s),
                11.5 * s,
                14. * s,
                muted,
            ));
        }
        for (y, label) in &f.splits {
            if label.is_empty() {
                continue;
            }
            let text = format!("[{label}]");
            let w = text_width(&text, 11.5) + 8.;
            layers.push(label_at(
                &[text],
                scaled([f.rect[0] + 8., *y + 2., w, 16.], s),
                11.5 * s,
                14. * s,
                muted,
            ));
        }
    }
    zstack(layers)
        .width(q.size[0] * s)
        .height(q.size[1] * s)
        .flow(Flow::TopLeft)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_flowchart_keeps_fanout_and_unicode_labels() {
        let graph = strict_flowchart("flowchart TB\n A[\"👤 User Browser\"]\n B[\"📱 Mobile Client\"]\n A & B --> C[Gateway]").unwrap();
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.id == "A" && n.label == "👤 User Browser")
        );
        assert!(graph.edges.iter().any(|e| e.from == "A" && e.to == "C"));
        assert!(graph.edges.iter().any(|e| e.from == "B" && e.to == "C"));
        assert!(!graph.nodes.iter().any(|n| n.id.contains('&')));
    }

    #[test]
    fn retained_canvas_splits_long_strokes_and_many_paths() {
        use volt_ui_kit::graphics::{MAX_CANVAS_PATHS, MAX_PATH_COMMANDS};
        let mut scene = Canvas::default();
        for _ in 0..MAX_CANVAS_PATHS + 2 {
            scene.stroke(VectorPath::rectangle(0., 0., 4., 4.), "@base-content", 1.);
        }
        let mut long = VectorPath::new().move_to(0., 0.);
        for i in 0..MAX_PATH_COMMANDS + 2 {
            long = long.line_to(i as f32, 1.);
        }
        scene.stroke(long, "@base-content", 1.);
        let layers = canvas_layers::<()>(scene, [2000., 20.]);
        assert!(layers.len() >= 2);
        for layer in layers {
            assert!(layer.build().is_ok());
        }
    }

    fn graph(src: &str) -> Graph {
        match build(src) {
            Diagram::Graph(g) => g,
            _ => panic!("not a graph: {src}"),
        }
    }

    fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    #[test]
    fn flowcharts_flow_in_their_direction_without_overlaps() {
        let g = graph(
            "graph TD\n  A[Start] --> B{Ready?}\n  B -->|yes| C[Ship it]\n  B -->|no| D[Fix bugs]\n  D --> B",
        );
        let at = |i: usize| g.nodes[i].rect;
        assert!(at(0)[1] < at(1)[1] && at(1)[1] < at(2)[1], "top to bottom");
        for i in 0..g.nodes.len() {
            for j in i + 1..g.nodes.len() {
                assert!(!overlap(at(i), at(j)), "nodes {i} and {j} overlap");
            }
        }
        // Labelled edges carry their labels between the layers.
        assert_eq!(g.edges.iter().filter(|w| w.label.is_some()).count(), 2);
        for w in g.edges.iter().filter_map(|w| w.label.as_ref()) {
            assert!(
                g.nodes.iter().all(|n| !overlap(n.rect, w.1)),
                "edge label overlaps a node"
            );
        }
        let lr = graph("flowchart LR\n  A --> B --> C");
        assert!(
            lr.nodes[0].rect[0] < lr.nodes[1].rect[0] && lr.nodes[1].rect[0] < lr.nodes[2].rect[0]
        );
        assert!(g.size[0] > 0. && g.size[1] > 0.);
    }

    #[test]
    fn placement_keeps_gaps_and_stays_close() {
        let p = place(&[0., 0., 0.], &[10., 10.]);
        assert_eq!(p, vec![-10., 0., 10.]);
        let p = place(&[0., 50., 100.], &[10., 10.]);
        assert_eq!(p, vec![0., 50., 100.]);
    }

    #[test]
    fn extended_mermaid_families_render_graphically() {
        for source in [
            "gantt\n  title Release plan\n  dateFormat YYYY-MM-DD\n  section Build\n  Implement :done, impl, 2026-10-01, 3d\n  Test :active, test, after impl, 2d\n  Ship :milestone, after test, 0d",
            "timeline\n  title Releases\n  2025 : Prototype\n  2026 : Ship",
            "mindmap\n  root((MD Star))\n    Reader\n    Editor",
            "journey\n  title Release\n  section Build\n    Code: 5: Dev\n    Test: 4: Dev",
        ] {
            let d = build(source);
            let Diagram::Image { image, .. } = &d else {
                panic!("expected graphical Mermaid: {source}");
            };
            assert!(
                image
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[..3] != [255, 255, 255]),
                "diagram must contain visible ink, not just an empty canvas"
            );
            assert!(image.width() as u64 * image.height() as u64 <= 4_000_000);
            assert!(size(&d).is_some());
            let _ = view(&d, 1., "Menlo", 12.);
        }
    }

    #[test]
    fn extended_diagrams_keep_errors_and_source_limits() {
        assert!(matches!(build("gantt\ninvalid task"), Diagram::Error(_)));
        assert!(matches!(
            build(&"x".repeat(64 * 1024 + 1)),
            Diagram::Error(_)
        ));
    }

    #[test]
    fn every_kind_draws_or_falls_back() {
        for src in [
            "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi\n  loop Every minute\n    Alice->>Alice: Think\n  end",
            "pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85",
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy\n  Busy --> [*]",
            "classDiagram\n  Animal <|-- Duck",
        ] {
            let d = build(src);
            assert!(!matches!(d, Diagram::Error(_)), "{src}");
            let _ = view(&d, 1., "Menlo", 12.);
        }
        assert!(matches!(build("not a diagram"), Diagram::Error(_)));
    }
}
