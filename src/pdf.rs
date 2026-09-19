// New (not from keypeek): renders one PDF page per layer using printpdf's
// classic low-level API. Text uses an embedded system TTF (DejaVu Sans)
// rather than printpdf's built-in Helvetica, since Helvetica is limited to
// Windows-1252 and can't render the arrow glyphs some keycap labels use.

use crate::layout_key::{BorderStyle, LayoutKey};
use crate::metrics::FontMetrics;
use crate::resolve::resolve_label;
use crate::types::{EncoderTile, KeyboardLayout};
use printpdf::path::PaintMode;
use printpdf::*;
use std::fs::File;
use std::io::{BufWriter, Cursor};

const UNIT_MM: f32 = 19.0; // one KLE unit ~ one physical keycap (19mm pitch)
// Real printers can't mark the outermost few mm of a page — this needs to
// clear that unprintable border with room to spare, not just look OK in a
// PDF viewer.
const MARGIN_MM: f32 = 15.0;
const GAP_MM: f32 = 1.5; // visual gap between adjacent keycaps
const HEADER_FONT_PT: f32 = 14.0;
const HEADER_MM: f32 = 12.0; // vertical space reserved for the header line
const STRIP_MM: f32 = 3.2; // height of the behavior/argument legend strips
// Bundled at compile time (see fonts/LICENSE — Bitstream Vera style,
// redistribution permitted) so the binary needs no system font installed
// and needs nothing external at runtime: same font on Linux/Windows/macOS.
const DEJAVU_SANS: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
/// Symbol for a transparent key (falls through to the layer below), same
/// idea as Vial's hollow down-pointing triangle.
const TRANSPARENT_SYMBOL: &str = "\u{25BD}";
/// Rotation direction icons for encoder tiles.
const ROTATE_CW: &str = "\u{21BB}";
const ROTATE_CCW: &str = "\u{21BA}";

#[allow(clippy::too_many_arguments)]
pub fn export(
    layout: &KeyboardLayout,
    layers: &[Vec<Vec<Option<LayoutKey>>>],
    encoders: &[Vec<(Option<LayoutKey>, Option<LayoutKey>)>],
    title: &str,
    out_path: &str,
    portrait: bool,
    layers_per_page: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let layers_per_page = layers_per_page.max(1);
    let metrics = FontMetrics::new(DEJAVU_SANS)?;

    let (dim_x, dim_y) = layout.get_dimensions();
    let block_h = HEADER_MM + dim_y * UNIT_MM; // one layer's header + keycap grid

    // The keyboard is always laid out and measured in its natural (wide)
    // orientation; `--portrait` rotates the finished canvas onto a tall
    // page via the page's CTM rather than re-deriving every coordinate.
    let canvas_w = MARGIN_MM * 2.0 + dim_x * UNIT_MM;
    let canvas_h = MARGIN_MM * 2.0 + (layers_per_page as f32) * block_h;
    let (page_w, page_h) = if portrait { (canvas_h, canvas_w) } else { (canvas_w, canvas_h) };

    let (doc, page1, layer1) = PdfDocument::new(title, Mm(page_w), Mm(page_h), "Page 1");
    let font = doc.add_external_font(Cursor::new(DEJAVU_SANS))?;

    for (page_idx, chunk) in layers.chunks(layers_per_page).enumerate() {
        let page_layer = if page_idx == 0 {
            doc.get_page(page1).get_layer(layer1)
        } else {
            let (page, pdf_layer) =
                doc.add_page(Mm(page_w), Mm(page_h), format!("Page {}", page_idx + 1));
            doc.get_page(page).get_layer(pdf_layer)
        };

        if portrait {
            // Turning the printed (portrait) page 90° clockwise should show
            // the keyboard upright in its natural wide orientation — so
            // rotate the whole canvas 90° counter-clockwise into the page.
            page_layer.set_ctm(CurTransMat::TranslateRotate(Mm(canvas_h).into(), Mm(0.0).into(), 90.0));
        }

        for (i, layer_keys) in chunk.iter().enumerate() {
            let layer_idx = page_idx * layers_per_page + i;
            // Reusing the single-block layout formula for each stacked
            // block: pretending the canvas is only as tall as "everything
            // from this block downward" reproduces the same per-block
            // spacing as a lone layer, just shifted down by the blocks
            // above it.
            let block_canvas_h = canvas_h - (i as f32) * block_h;

            draw_header(&page_layer, &font, title, layer_idx, block_canvas_h);

            for key in &layout.keys {
                let resolved_key = layer_keys
                    .get(key.row)
                    .and_then(|r| r.get(key.col))
                    .and_then(|k| k.as_ref());

                draw_key(
                    &page_layer,
                    &font,
                    &metrics,
                    key.x,
                    key.y,
                    key.w,
                    key.h,
                    block_canvas_h,
                    resolved_key,
                );
            }

            for tile in &layout.encoders {
                let resolved_key = encoders
                    .get(layer_idx)
                    .and_then(|layer_encoders| layer_encoders.get(tile.id as usize))
                    .map(|(ccw, cw)| if tile.clockwise { cw } else { ccw })
                    .and_then(|k| k.as_ref());

                draw_encoder(&page_layer, &font, &metrics, tile, block_canvas_h, resolved_key);
            }
        }
    }

    doc.save(&mut BufWriter::new(File::create(out_path)?))?;
    Ok(())
}

fn draw_header(
    page_layer: &PdfLayerReference,
    font: &IndirectFontRef,
    title: &str,
    layer_idx: usize,
    canvas_h: f32,
) {
    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
    // Baseline sits far enough below the margin line that the text's cap
    // height doesn't poke back up past it.
    let cap_height_mm = HEADER_FONT_PT * 0.72 * 0.3527778;
    page_layer.use_text(
        format!("{title} — Layer {layer_idx}"),
        HEADER_FONT_PT,
        Mm(MARGIN_MM),
        Mm(canvas_h - MARGIN_MM - cap_height_mm),
        font,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_key(
    page_layer: &PdfLayerReference,
    font: &IndirectFontRef,
    metrics: &FontMetrics,
    kle_x: f32,
    kle_y: f32,
    kle_w: f32,
    kle_h: f32,
    canvas_h_mm: f32,
    key: Option<&LayoutKey>,
) {
    let x0 = MARGIN_MM + kle_x * UNIT_MM + GAP_MM * 0.5;
    let w = kle_w * UNIT_MM - GAP_MM;
    let h = kle_h * UNIT_MM - GAP_MM;
    // KLE y grows downward from the top; PDF y grows upward from the bottom.
    let top_y_mm = canvas_h_mm - MARGIN_MM - HEADER_MM - kle_y * UNIT_MM - GAP_MM * 0.5;
    let y0 = top_y_mm - h;

    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.95, 0.95, 0.95, None)));
    draw_border(page_layer, key.map(|k| k.border).unwrap_or(BorderStyle::None));
    let rect = Rect::new(Mm(x0), Mm(y0), Mm(x0 + w), Mm(y0 + h)).with_mode(PaintMode::FillStroke);
    page_layer.add_rect(rect);
    // Reset to solid for whatever draws next (text uses no outline, but stay tidy).
    page_layer.set_line_dash_pattern(LineDashPattern::default());

    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));

    let Some(key) = key else {
        draw_centered(page_layer, font, metrics, TRANSPARENT_SYMBOL, x0, y0, w, h, 9.0);
        return;
    };

    let has_top_strip = key.behavior.is_some();
    let has_bottom_strip = key.argument.is_some();

    if let Some(behavior) = &key.behavior {
        let text = behavior.short.as_deref().unwrap_or(&behavior.full);
        draw_strip_text(page_layer, font, metrics, text, x0, y0 + h - STRIP_MM, w, STRIP_MM);
    }
    if let Some(argument) = &key.argument {
        let text = argument.short.as_deref().unwrap_or(&argument.full);
        draw_strip_text(page_layer, font, metrics, text, x0, y0, w, STRIP_MM);
    }

    let mid_y0 = y0 + if has_bottom_strip { STRIP_MM } else { 0.0 };
    let mid_h = h - if has_top_strip { STRIP_MM } else { 0.0 } - if has_bottom_strip { STRIP_MM } else { 0.0 };

    let resolved = resolve_label(key);
    let max_w = w - 0.16 * UNIT_MM;
    let base_size = (mid_h * 0.46 * 2.8346).clamp(6.0, 13.0);

    // Prefer the full label; fall back to the short form, then shrink font
    // size, so long legends (e.g. "Page Up") never spill past their keycap.
    let (text, mut size) = pick_fitting(metrics, &resolved.full, resolved.short.as_deref(), max_w, base_size);
    while size > 4.0 && metrics.width_mm(text, size) > max_w {
        size -= 0.5;
    }

    draw_centered(page_layer, font, metrics, text, x0, mid_y0, w, mid_h, size);
}

#[allow(clippy::too_many_arguments)]
fn draw_encoder(
    page_layer: &PdfLayerReference,
    font: &IndirectFontRef,
    metrics: &FontMetrics,
    tile: &EncoderTile,
    canvas_h_mm: f32,
    key: Option<&LayoutKey>,
) {
    let x0 = MARGIN_MM + tile.x * UNIT_MM + GAP_MM * 0.5;
    let w = tile.w * UNIT_MM - GAP_MM;
    let h = tile.h * UNIT_MM - GAP_MM;
    let top_y_mm = canvas_h_mm - MARGIN_MM - HEADER_MM - tile.y * UNIT_MM - GAP_MM * 0.5;
    let y0 = top_y_mm - h;

    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.95, 0.95, 0.95, None)));
    draw_border(page_layer, key.map(|k| k.border).unwrap_or(BorderStyle::None));
    let rect = Rect::new(Mm(x0), Mm(y0), Mm(x0 + w), Mm(y0 + h)).with_mode(PaintMode::FillStroke);
    page_layer.add_rect(rect);
    page_layer.set_line_dash_pattern(LineDashPattern::default());

    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));

    let arrow = if tile.clockwise { ROTATE_CW } else { ROTATE_CCW };
    draw_strip_text(page_layer, font, metrics, arrow, x0, y0 + h - STRIP_MM, w, STRIP_MM);

    let mid_y0 = y0;
    let mid_h = h - STRIP_MM;

    let Some(key) = key else {
        draw_centered(page_layer, font, metrics, TRANSPARENT_SYMBOL, x0, mid_y0, w, mid_h, 9.0);
        return;
    };

    let resolved = resolve_label(key);
    let max_w = w - 0.16 * UNIT_MM;
    let base_size = (mid_h * 0.46 * 2.8346).clamp(6.0, 13.0);
    let (text, mut size) = pick_fitting(metrics, &resolved.full, resolved.short.as_deref(), max_w, base_size);
    while size > 4.0 && metrics.width_mm(text, size) > max_w {
        size -= 0.5;
    }

    draw_centered(page_layer, font, metrics, text, x0, mid_y0, w, mid_h, size);
}

fn pick_fitting<'a>(
    metrics: &FontMetrics,
    full: &'a str,
    short: Option<&'a str>,
    max_w: f32,
    size: f32,
) -> (&'a str, f32) {
    if metrics.width_mm(full, size) <= max_w {
        return (full, size);
    }
    if let Some(short) = short {
        if metrics.width_mm(short, size) <= max_w {
            return (short, size);
        }
    }
    (short.unwrap_or(full), size)
}

fn draw_border(page_layer: &PdfLayerReference, style: BorderStyle) {
    match style {
        BorderStyle::None => {
            page_layer.set_outline_color(Color::Rgb(Rgb::new(0.4, 0.4, 0.4, None)));
            page_layer.set_outline_thickness(0.6);
        }
        BorderStyle::Solid => {
            page_layer.set_outline_color(Color::Rgb(Rgb::new(0.15, 0.25, 0.55, None)));
            page_layer.set_outline_thickness(1.6);
        }
        BorderStyle::Dashed => {
            page_layer.set_outline_color(Color::Rgb(Rgb::new(0.15, 0.25, 0.55, None)));
            page_layer.set_outline_thickness(1.2);
            page_layer.set_line_dash_pattern(LineDashPattern {
                dash_1: Some(3),
                gap_1: Some(2),
                ..Default::default()
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_centered(
    page_layer: &PdfLayerReference,
    font: &IndirectFontRef,
    metrics: &FontMetrics,
    text: &str,
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
    font_size: f32,
) {
    if text.is_empty() {
        return;
    }
    let text_w = metrics.width_mm(text, font_size);
    let cap_height_mm = font_size * 0.72 * 0.3527778; // rough visual cap height for vertical centering
    let x = x0 + (w - text_w) * 0.5;
    let y = y0 + (h - cap_height_mm) * 0.5;
    page_layer.use_text(text, font_size, Mm(x), Mm(y), font);
}

fn draw_strip_text(
    page_layer: &PdfLayerReference,
    font: &IndirectFontRef,
    metrics: &FontMetrics,
    text: &str,
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
) {
    let mut size = 7.0;
    while size > 4.5 && metrics.width_mm(text, size) > w - 1.0 {
        size -= 0.5;
    }
    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.35, 0.35, 0.35, None)));
    draw_centered(page_layer, font, metrics, text, x0, y0, w, h, size);
    page_layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
}
