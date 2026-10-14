//! A document as PDF pages, drawn by the app's own renderer: Quick Look
//! previews and Export as PDF look exactly like the reader.
//!
//! The document is laid out once at the page width, broken into pages
//! between rows (blocks, list items, table and code runs), and each page is
//! rasterized and embedded as a compressed image.

use std::path::Path;

use volt_ui_kit::{
    UiTheme, Viewport,
    declarative::{Bindings, Runtime},
    native::RasterPainter,
    ui::prelude::*,
};

use crate::{
    App,
    app::{self, Doc},
    render::{self, Ctx},
    settings::{Appearance, Settings},
    view,
    workspace::FileKind,
};

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Page width and the tallest page, in points (one point per pixel of
    /// the reader at 100%).
    pub width: f32,
    pub page: f32,
    /// Pixels per point in the embedded images.
    pub scale: f32,
    pub dark: bool,
    /// Stop after this many pages (a Quick Look preview); the last page then
    /// says so. Exports take every page.
    pub max_pages: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 820.,
            page: 1400.,
            scale: 2.,
            dark: false,
            max_pages: usize::MAX,
        }
    }
}

/// The file at `path` as a PDF.
pub fn pdf(path: &Path, o: Options) -> Result<Vec<u8>, String> {
    let doc = app::load(path)?;
    let pages = pages(&doc, o)?;
    Ok(write_pdf(&pages, o.width))
}

/// A rendered page: its height in points and RGB pixels.
struct Page {
    height: f32,
    size: [u32; 2],
    rgb: Vec<u8>,
}

fn pages(doc: &Doc, o: Options) -> Result<Vec<Page>, String> {
    let settings = Settings {
        appearance: if o.dark {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        ..Settings::default()
    };
    let mut app = App::new(settings, None);
    app.page_width = Some(o.width);
    let ui = UiTheme::from_palette(app.theme().palette(None));
    let background = ui.resolve("@base-100").unwrap_or([1., 1., 1., 1.]);
    let units = doc.units();
    let keys: Vec<String> = (0..units.len())
        .map(|i| render::unit_key(&units, i))
        .collect();
    let ctx = Ctx {
        images: &doc.images,
        dark: o.dark,
        code_view: matches!(doc.kind, FileKind::Code(language) if language != "csv"),
        copied: None,
        typo: view::typography(&app),
        find: None,
        menu: None,
        controls: false,
    };
    // Rows `range` of the document at their natural heights: inside a
    // scroll view nothing is squeezed to fit the viewport.
    let page = |range: std::ops::Range<usize>| -> Element<App> {
        let rows: Vec<Element<App>> = range.map(|i| render::row(&units, i, &ctx)).collect();
        vstack![vstack(rows).key("export-rows")]
            .key("export")
            .width(o.width)
            .scroll(Scroll::Vertical)
            .scrollbars(Scrollbars::Hidden)
            .into()
    };
    let layout = |element: Element<App>, height: f32, scale: f32, painter: &mut RasterPainter| {
        let (tree, handlers) = element.build().map_err(|e| e.to_string())?.into_parts();
        let mut runtime =
            Runtime::new(tree, &handlers, &Bindings::default()).map_err(|e| e.to_string())?;
        let viewport =
            Viewport::new(o.width * scale, height * scale, scale).ok_or("invalid page size")?;
        painter.set_viewport(viewport);
        runtime
            .layout(viewport, &Bindings::default(), painter)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(runtime)
    };

    // Measure every row. The kit lays out only what falls inside the
    // viewport, and viewports have a size limit, so measure in batches:
    // each records the rows it laid out and the next starts after them.
    let mut painter = RasterPainter::new(
        Viewport::new(o.width * o.scale, o.page * o.scale, o.scale).ok_or("invalid page size")?,
    );
    let tallest = MAX_PIXELS / o.scale;
    let mut rows = vec![0f32; units.len()];
    let mut next = 0;
    // Enough for the page limit; later rows are not shown.
    let wanted = o.page * o.max_pages as f32;
    let mut measured = 0.;
    while next < units.len() && measured < wanted {
        let batch = next..units.len().min(next + MEASURE_ROWS);
        let runtime = layout(page(batch.clone()), tallest, o.scale, &mut painter)?;
        let heights: std::collections::HashMap<&str, f32> = runtime
            .boxes()
            .iter()
            .map(|b| (b.id.as_str(), b.bounds.bounds()[3] / o.scale))
            .collect();
        let start = next;
        while let Some(h) = keys[..batch.end]
            .get(next)
            .and_then(|k| heights.get(k.as_str()))
        {
            rows[next] = *h;
            measured += *h;
            next += 1;
        }
        if next == start {
            next += 1; // A row with no box takes no room.
        }
    }

    // Each page lays out and draws only its own rows; a page taller than
    // the pixel limit (one huge row) is drawn at a lower scale.
    let mut out = Vec::new();
    let headings: Vec<bool> = (0..units.len())
        .map(|i| units[i].heading().is_some())
        .collect();
    let mut draw = |element: Element<App>, height: f32| -> Result<Page, String> {
        let scale = o.scale.min(MAX_PIXELS / height).max(0.25);
        let runtime = layout(element, height, scale, &mut painter)?;
        let batches = runtime.paint_ui(&mut painter, &ui);
        let pixmap = painter.rasterize(&batches, background)?;
        let rgb: Vec<u8> = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        Ok(Page {
            height,
            size: [pixmap.width(), pixmap.height()],
            rgb,
        })
    };
    let ranges = page_ranges(&rows, &headings, o.page, o.max_pages);
    let shown = ranges.last().map_or(0, |r| r.end);
    for range in ranges {
        let height = rows[range.clone()].iter().sum::<f32>().max(1.);
        out.push(draw(page(range), height)?);
    }
    // A preview cut short says so rather than just stopping.
    if shown < units.len() {
        let note = format!(
            "This preview shows the first {} pages. Open the file in MD Star to read the rest.",
            out.len()
        );
        let element = vstack![text(note).font_size(13.).color(ColorToken::Muted)]
            .key("export-more")
            .width(o.width)
            .padding(render::margin_x(Some(o.width)), Edge::Horizontal)
            .padding(28., Edge::Vertical)
            .into();
        out.push(draw(element, 80.)?);
    }
    Ok(out)
}

/// The kit's largest viewport side, in pixels.
const MAX_PIXELS: f32 = 16384.;
/// Rows laid out per measuring pass: more than fit in one viewport.
const MEASURE_ROWS: usize = 500;

/// Rows (by height) grouped into pages at most `page` tall. Rows are never
/// cut: a row taller than a page gets a page of its own, as tall as it is.
/// A heading never ends a page; it moves on with the block it introduces.
fn page_ranges(
    rows: &[f32],
    headings: &[bool],
    page: f32,
    max_pages: usize,
) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < rows.len() && out.len() < max_pages {
        let mut end = start + 1;
        let mut height = rows[start];
        while end < rows.len() && height + rows[end] <= page {
            height += rows[end];
            end += 1;
        }
        while end < rows.len() && end - start > 1 && headings.get(end - 1) == Some(&true) {
            end -= 1;
        }
        out.push(start..end);
        start = end;
    }
    if rows.is_empty() {
        out.push(0..0);
    }
    out
}

/// A minimal PDF: one image per page, scaled to the page, Flate compressed.
fn write_pdf(pages: &[Page], width: f32) -> Vec<u8> {
    let mut pdf: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let object = |pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    // Objects: 1 catalog, 2 pages, then per page: page, contents, image.
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", 3 + i * 3))
        .collect();
    object(&mut pdf, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    object(
        &mut pdf,
        &mut offsets,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .as_bytes(),
    );
    for (i, page) in pages.iter().enumerate() {
        let (contents, image) = (4 + i * 3, 5 + i * 3);
        object(
            &mut pdf,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {h}] \
                 /Resources << /XObject << /Im0 {image} 0 R >> >> /Contents {contents} 0 R >>",
                h = page.height
            )
            .as_bytes(),
        );
        let draw = format!("q {width} 0 0 {} 0 0 cm /Im0 Do Q", page.height);
        object(
            &mut pdf,
            &mut offsets,
            format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).as_bytes(),
        );
        let data = miniz_oxide::deflate::compress_to_vec_zlib(&page.rgb, 6);
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            page.size[0],
            page.size[1],
            data.len()
        )
        .into_bytes();
        body.extend_from_slice(&data);
        body.extend_from_slice(b"\nendstream");
        object(&mut pdf, &mut offsets, &body);
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_hold_whole_rows_and_cover_everything() {
        let rows = vec![100.; 30];
        let plain = vec![false; 30];
        let pages = page_ranges(&rows, &plain, 1000., 60);
        assert_eq!(pages.first().unwrap().start, 0);
        assert_eq!(pages.last().unwrap().end, 30);
        for w in pages.windows(2) {
            assert_eq!(w[0].end, w[1].start, "pages meet");
        }
        assert!(
            pages
                .iter()
                .all(|r| rows[r.clone()].iter().sum::<f32>() <= 1000.)
        );
        // A row taller than a page is never cut: it gets its own page.
        assert_eq!(
            page_ranges(&[300., 2500., 300.], &plain, 1000., 60),
            [0..1, 1..2, 2..3]
        );
        assert_eq!(page_ranges(&rows, &plain, 100., 3).len(), 3, "page limit");
        assert_eq!(
            page_ranges(&[], &[], 1000., 60),
            vec![std::ops::Range { start: 0, end: 0 }]
        );
        // A heading that would end a page starts the next one instead.
        let mut headings = plain.clone();
        headings[9] = true;
        let pages = page_ranges(&rows, &headings, 1000., 60);
        assert_eq!(pages[0], 0..9);
        assert_eq!(pages[1].start, 9);
    }

    #[test]
    fn every_page_has_content_and_the_pages_cover_the_document() {
        let dir = std::env::temp_dir().join(format!("mdstar-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.md");
        let mut text = String::from(
            "# Title\n\nSome **text** with $E = mc^2$ :rocket:.\n\n```mermaid\ngraph TD\n  A --> B\n```\n",
        );
        for i in 0..80 {
            text.push_str(&format!(
                "\nParagraph {i} with enough words to wrap across the page width a little.\n"
            ));
            if i % 20 == 0 {
                text.push_str("\n```text\none\ntwo\nthree\n```\n");
            }
        }
        text.push_str("\n## The end\n\nLast words.\n");
        std::fs::write(&path, text).unwrap();
        let doc = app::load(&path).unwrap();
        let pages = pages(&doc, Options::default()).unwrap();
        let bytes = pdf(&path, Options::default()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            pages.len() >= 3,
            "a long document spans pages: {}",
            pages.len()
        );
        // 80 paragraphs of at least a line each, plus headings and code.
        let total: f32 = pages.iter().map(|p| p.height).sum();
        assert!(total > 80. * 28., "pages cover the document: {total}");
        for (i, p) in pages.iter().enumerate() {
            let paper = [p.rgb[0], p.rgb[1], p.rgb[2]];
            let ink = p
                .rgb
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|px| **px != paper)
                .count();
            assert!(ink > 1000, "page {i} is blank");
        }
        // Every page by default; a limited preview ends with a note.
        assert!(
            pages.iter().all(|p| p.height > 80.),
            "no note on a full export"
        );
        let limited = Options {
            max_pages: 2,
            ..Options::default()
        };
        let short = super::pages(&doc, limited).unwrap();
        assert_eq!(short.len(), 3, "two pages and the note");
        assert_eq!(short[2].height, 80.);
        assert_eq!(&bytes[..9], b"%PDF-1.4\n");
        assert_eq!(&bytes[bytes.len() - 6..], b"%%EOF\n");
    }
}
