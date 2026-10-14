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
    /// Stop after this many pages; long documents end with a note.
    pub max_pages: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 820.,
            page: 1400.,
            scale: 2.,
            dark: false,
            max_pages: 60,
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
    let build = |offset: f32| -> Element<App> {
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
        let rows: Vec<Element<App>> = (0..units.len())
            .map(|i| render::row(&units, i, &ctx))
            .collect();
        // Inside a scroll view rows keep their natural heights; the page
        // shows the slice starting at `offset`.
        vstack![vstack(rows).key("export-rows").offset(0., -offset)]
            .key("export")
            .width(o.width)
            .scroll(Scroll::Vertical)
            .scrollbars(Scrollbars::Hidden)
            .into()
    };
    let layout = |offset: f32, height: f32, painter: &mut RasterPainter| {
        let (tree, handlers) = build(offset)
            .build()
            .map_err(|e| e.to_string())?
            .into_parts();
        let mut runtime =
            Runtime::new(tree, &handlers, &Bindings::default()).map_err(|e| e.to_string())?;
        let viewport = Viewport::new(o.width * o.scale, height * o.scale, o.scale)
            .ok_or("invalid page size")?;
        painter.set_viewport(viewport);
        runtime
            .layout(viewport, &Bindings::default(), painter)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(runtime)
    };

    // Where each row starts and ends, to break pages between rows.
    let mut painter = RasterPainter::new(
        Viewport::new(o.width * o.scale, o.page * o.scale, o.scale).ok_or("invalid page size")?,
    );
    let runtime = layout(0., o.page, &mut painter)?;
    let rows: Vec<[f32; 2]> = keys
        .iter()
        .filter_map(|key| {
            let b = runtime.boxes().iter().find(|b| &b.id == key)?;
            let [_, y, _, h] = b.bounds.bounds();
            Some([y / o.scale, (y + h) / o.scale])
        })
        .collect();
    let breaks = page_breaks(&rows, o.page, o.max_pages);

    let mut out = Vec::new();
    for [top, bottom] in breaks {
        let height = (bottom - top).max(1.);
        let runtime = layout(top, height, &mut painter)?;
        let batches = runtime.paint_ui(&mut painter, &ui);
        let pixmap = painter.rasterize(&batches, background)?;
        let rgb: Vec<u8> = pixmap
            .data()
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        out.push(Page {
            height,
            size: [pixmap.width(), pixmap.height()],
            rgb,
        });
    }
    Ok(out)
}

/// Page spans [top, bottom] covering the rows, each at most `page` tall and
/// breaking between rows; a row taller than a page is cut across pages.
fn page_breaks(rows: &[[f32; 2]], page: f32, max_pages: usize) -> Vec<[f32; 2]> {
    let end = rows.iter().map(|r| r[1]).fold(0., f32::max).max(1.);
    let mut out = Vec::new();
    let mut top = 0.;
    while top < end && out.len() < max_pages {
        let limit = top + page;
        if limit >= end {
            out.push([top, end]);
            break;
        }
        // The last row start that leaves the page at least half full.
        let cut = rows
            .iter()
            .map(|r| r[0])
            .filter(|y| *y > top + page * 0.5 && *y <= limit)
            .fold(None, |best: Option<f32>, y| {
                Some(best.map_or(y, |b| b.max(y)))
            })
            .unwrap_or(limit);
        out.push([top, cut]);
        top = cut;
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
    fn pages_break_between_rows_and_cover_everything() {
        let rows: Vec<[f32; 2]> = (0..30)
            .map(|i| [i as f32 * 100., i as f32 * 100. + 90.])
            .collect();
        let pages = page_breaks(&rows, 1000., 60);
        assert_eq!(pages.first().unwrap()[0], 0.);
        assert_eq!(pages.last().unwrap()[1], 2990.);
        for w in pages.windows(2) {
            assert_eq!(w[0][1], w[1][0], "pages meet");
            assert!(
                rows.iter().any(|r| r[0] == w[0][1]),
                "breaks fall between rows"
            );
        }
        assert!(pages.iter().all(|p| p[1] - p[0] <= 1000.));
        // A row taller than a page is cut.
        let tall = page_breaks(&[[0., 2500.]], 1000., 60);
        assert_eq!(tall, [[0., 1000.], [1000., 2000.], [2000., 2500.]]);
        assert_eq!(page_breaks(&rows, 100., 3).len(), 3, "page limit");
    }

    #[test]
    fn a_document_becomes_a_valid_pdf() {
        let dir = std::env::temp_dir().join(format!("mdstar-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.md");
        let mut text = String::from(
            "# Title\n\nSome **text** with $E = mc^2$ :rocket:.\n\n```mermaid\ngraph TD\n  A --> B\n```\n",
        );
        for i in 0..60 {
            text.push_str(&format!(
                "\nParagraph {i} with enough words to wrap across the page width a little.\n"
            ));
        }
        std::fs::write(&path, text).unwrap();
        let bytes = pdf(&path, Options::default()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let head = String::from_utf8_lossy(&bytes[..9]);
        assert_eq!(head, "%PDF-1.4\n");
        let tail = String::from_utf8_lossy(&bytes[bytes.len() - 6..]);
        assert_eq!(tail, "%%EOF\n");
        let text = String::from_utf8_lossy(&bytes);
        let count: usize = text
            .split("/Count ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap();
        assert!(count >= 2, "a long document spans pages: {count}");
    }
}
