//! Tests for the controlled GFM table renderer.
//!
//! Width assertions go through `fit`, which is the layout decision. A rendered
//! rect cannot serve that purpose: a horizontal `ScrollArea` reports its
//! viewport rather than its content, so a rect-based bound would pass even when
//! the table overflows.

use super::{Cell, Need, fit, parse, ranges};
use egui_commonmark::CommonMarkCache;

const TABLE: &str = "\
| Option | Upside | Downside | Reversibility | Verdict |
|--------|--------|----------|---------------|---------|
| **A. Start six-week trial now** | Fast learning | Unmeasured baseline; coverage and hidden-overtime risk | High | Reject |
| **B. Baseline then trial** | Bounded; attributable results | Slower start | High | **Selected** |
| C. Delay indefinitely | Avoids risk | No learning | High | Reject unless veto |
| D. Do nothing | No disruption | No evidence | High | Not preferred |
";

const SPACING: f32 = 10.0;

fn plain(cell: &Cell) -> String {
    cell.spans.iter().map(|(text, _)| text.as_str()).collect()
}

fn need(intrinsic: f32, word: f32) -> Need {
    Need { intrinsic, word }
}

/// Five prose columns shaped like the real brief.
fn brief_needs() -> Vec<Need> {
    vec![
        need(360.0, 90.0),
        need(190.0, 70.0),
        need(430.0, 110.0),
        need(200.0, 95.0),
        need(300.0, 60.0),
    ]
}

fn total(widths: &[f32]) -> f32 {
    widths.iter().sum::<f32>() + SPACING * widths.len().saturating_sub(1) as f32
}

fn assert_fits(widths: &[f32], available: f32) {
    let used = total(widths);
    assert!(
        used <= available + 0.5,
        "columns overflow: {used} > {available}"
    );
}

/// Render a document at a fixed width and report the settled content height.
fn settled_height(text: &str, width: f32) -> f32 {
    let ctx = egui::Context::default();
    let mut cache = CommonMarkCache::default();
    let mut height = 0.0_f32;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 3000.0),
        )),
        ..Default::default()
    };
    for _ in 0..5 {
        let mut output = ctx.run_ui(input.clone(), |ui| {
            ui.set_width(width);
            super::super::reader::body(ui, text, &mut cache);
            height = ui.min_rect().height();
        });
        output.textures_delta.clear();
    }
    height
}

#[test]
fn ranges_find_tables_only_at_top_level() {
    let doc = format!("Intro.\n\n{TABLE}\n\nAfter.\n");
    let found = ranges(&doc);
    assert_eq!(found.len(), 1);
    assert!(doc[found[0].0..found[0].1].starts_with("| Option |"));
    assert!(
        doc[found[0].0..found[0].1]
            .trim_end()
            .ends_with("Not preferred |")
    );
    assert!(ranges("No tables here.\n").is_empty());
}

#[test]
fn parse_keeps_header_rows_and_inline_styling() {
    let table = parse(TABLE).expect("table parses");
    assert_eq!(table.header.len(), 5);
    assert_eq!(plain(&table.header[0]), "Option");
    assert_eq!(table.rows.len(), 4);
    assert_eq!(plain(&table.rows[0][0]), "A. Start six-week trial now");
    assert!(
        table.rows[0][0].spans.iter().any(|(_, f)| f.strong),
        "bold cell text lost its emphasis"
    );
}

/// The reported bug: a five-column brief must never overflow the reader. When
/// the columns fit, they shrink to fit; the sum never exceeds the width.
#[test]
fn five_column_brief_fits_a_wide_reader() {
    let widths = fit(&brief_needs(), 820.0, SPACING).expect("must not stack");
    assert_eq!(widths.len(), 5);
    assert_fits(&widths, 820.0);
    assert!(widths.iter().all(|w| *w > 0.0), "a column vanished");
}

/// At the screenshot's width five prose columns cannot stay readable. The
/// decided behaviour is the stacked layout, never a horizontal scroller.
#[test]
fn five_column_brief_stacks_on_a_narrow_reader() {
    assert_eq!(fit(&brief_needs(), 420.0, SPACING), None);
}

#[test]
fn columns_shrink_proportionally_before_any_floor() {
    let needs = [need(300.0, 40.0), need(300.0, 40.0), need(300.0, 40.0)];
    let widths = fit(&needs, 620.0, SPACING).expect("must not stack");
    assert_fits(&widths, 620.0);
    // 620 - 20 spacing = 600 across three equal columns.
    assert!((widths[0] - 200.0).abs() < 0.5, "got {widths:?}");
}

/// A column never shrinks below its longest word, so words are not broken
/// mid-glyph ("Reversibili / ty").
#[test]
fn columns_never_break_a_word() {
    let needs = [need(600.0, 150.0), need(600.0, 40.0)];
    let widths = fit(&needs, 400.0, SPACING).expect("must not stack");
    assert!(widths[0] >= 150.0, "word split: {widths:?}");
    assert_fits(&widths, 400.0);
}

#[test]
fn wide_reader_keeps_intrinsic_widths() {
    let needs = [need(120.0, 60.0), need(80.0, 30.0)];
    assert_eq!(fit(&needs, 820.0, SPACING), Some(vec![120.0, 80.0]));
}

#[test]
fn every_layout_fits_or_stacks() {
    for available in [320.0_f32, 420.0, 560.0, 640.0, 820.0] {
        match fit(&brief_needs(), available, SPACING) {
            Some(widths) => assert_fits(&widths, available),
            None => assert!(available < 820.0, "stacked at a workable width"),
        }
    }
}

#[test]
fn table_renders_rows_and_keeps_prose() {
    let with_table = settled_height(&format!("Intro.\n\n{TABLE}\n\nAfter.\n"), 820.0);
    let without = settled_height("Intro.\n\nAfter.\n", 820.0);
    assert!(
        with_table > without + 40.0,
        "table rendered no rows: {with_table} vs {without}"
    );
}

#[test]
fn narrow_reader_renders_stacked_rows() {
    let narrow = settled_height(&format!("Intro.\n\n{TABLE}\n\nAfter.\n"), 420.0);
    let wide = settled_height(&format!("Intro.\n\n{TABLE}\n\nAfter.\n"), 820.0);
    assert!(
        narrow > wide,
        "stacked layout should be taller than columns: {narrow} vs {wide}"
    );
}

/// Collect the text actually painted for a document, as `(text, y)`.
fn painted_texts(text: &str, width: f32) -> Vec<(String, f32)> {
    use egui::epaint::Shape;
    let ctx = egui::Context::default();
    let mut cache = CommonMarkCache::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 4000.0),
        )),
        ..Default::default()
    };
    let mut found = Vec::new();
    for pass in 0..5 {
        let mut output = ctx.run_ui(input.clone(), |ui| {
            crate::ui::theme::Theme::default().apply(ui.ctx());
            crate::ui::reader::document(ui, ("brief", width as i32), text, &mut cache);
        });
        output.textures_delta.clear();
        if pass == 4 {
            found = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    Shape::Text(t) => Some((t.galley.text().to_string(), t.pos.y)),
                    _ => None,
                })
                .collect();
        }
    }
    found
}

/// Both layouts must show the column headings. A band with no labels tells the
/// reader nothing about what each value means.
#[test]
fn every_layout_shows_the_column_headings() {
    let wide = painted_texts(TABLE, 900.0);
    let narrow = painted_texts(TABLE, 400.0);
    for headings in [&wide, &narrow] {
        for expected in ["Option", "Upside", "Downside", "Reversibility", "Verdict"] {
            assert!(
                headings.iter().any(|(text, _)| text.trim() == expected),
                "heading {expected:?} missing; painted: {:?}",
                headings
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .take(20)
                    .collect::<Vec<_>>()
            );
        }
    }
}

/// The stacked layout must still show the row's first cell, which is its title.
#[test]
fn stacked_layout_keeps_every_row_title() {
    let narrow = painted_texts(TABLE, 400.0);
    for title in [
        "A. Start six-week trial now",
        "B. Baseline then trial",
        "C. Delay indefinitely",
        "D. Do nothing",
    ] {
        assert!(
            narrow.iter().any(|(text, _)| text.contains(title)),
            "row title {title:?} missing from the stacked layout"
        );
    }
}
