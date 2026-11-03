//! GFM tables rendered with explicit column widths and a distinct header.
//!
//! `egui_commonmark` draws tables as an `egui::Grid` sized to its unwrapped
//! content, so a five-column brief is wider than the reader, and its parser
//! discards `Tag::TableHead` (`pulldown.rs:1420`), leaving the header row
//! identical to body rows. Both need column widths and header styling the crate
//! cannot express, so the reader splits the document on table spans and renders
//! tables here while the crate renders everything else.
//!
//! A table never scrolls horizontally. Columns shrink proportionally down to
//! the width of their longest word, and when even that will not fit, the table
//! switches to a stacked label/value layout.

use egui::{Align, Frame, Layout, RichText, Shape, TextStyle, Ui, Vec2};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

const CELL_PAD_X: f32 = 8.0;
const CELL_PAD_Y: f32 = 6.0;

/// Upper bound on the per-column minimum derived from the longest word. A
/// pathological token must not force every table to stack.
const MAX_WORD_MIN: f32 = 220.0;

#[derive(Default, Clone, Copy)]
struct Format {
    strong: bool,
    emphasis: bool,
    code: bool,
    strikethrough: bool,
}

#[derive(Default)]
pub struct Cell {
    spans: Vec<(String, Format)>,
}

impl Cell {
    fn plain(&self) -> String {
        self.spans.iter().map(|(text, _)| text.as_str()).collect()
    }

    fn is_empty(&self) -> bool {
        self.plain().trim().is_empty()
    }

    /// Inline spans as widgets. A wrapping horizontal layout lets a cell wrap
    /// across spans, which one label cannot do while keeping bold and code.
    fn show(&self, ui: &mut Ui, strong: bool) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (text, format) in &self.spans {
                let mut rich = RichText::new(text);
                if strong || format.strong {
                    rich = rich.strong();
                }
                if format.emphasis {
                    rich = rich.italics();
                }
                if format.code {
                    rich = rich.code();
                }
                if format.strikethrough {
                    rich = rich.strikethrough();
                }
                ui.add(egui::Label::new(rich).wrap());
            }
        });
    }
}

pub struct Table {
    header: Vec<Cell>,
    rows: Vec<Vec<Cell>>,
}

#[derive(Debug, PartialEq)]
enum Plan {
    Columns(Vec<f32>),
    Stacked,
}

/// What one column needs: its full single-line width and the width of its
/// longest unbreakable word.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Need {
    intrinsic: f32,
    word: f32,
}

/// Byte ranges of the top-level tables, in document order. A `Start(Table)`
/// span covers the whole table, delimiter row included.
pub fn ranges(text: &str) -> Vec<(usize, usize)> {
    let mut depth = 0usize;
    let mut found = Vec::new();
    for (event, span) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        match event {
            Event::Start(Tag::Table(_)) if depth == 0 => found.push((span.start, span.end)),
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    found
}

/// Read one table from Markdown that begins at its first `|` line.
pub fn parse(source: &str) -> Option<Table> {
    let mut events = Parser::new_ext(source, Options::ENABLE_TABLES).into_offset_iter();
    if !matches!(events.next()?.0, Event::Start(Tag::Table(_))) {
        return None;
    }
    let mut builder = Builder::default();
    for (event, _) in events {
        builder.event(event);
    }
    if builder.header.is_empty() {
        return None;
    }
    Some(Table {
        header: builder.header,
        rows: builder.rows,
    })
}

/// Accumulates cells as the parser walks a table.
#[derive(Default)]
struct Builder {
    header: Vec<Cell>,
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Cell,
    format: Format,
}

impl Builder {
    /// Cell and row boundaries.
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::TableCell) => self.cell = Cell::default(),
            Event::End(TagEnd::TableCell) => self.row.push(std::mem::take(&mut self.cell)),
            Event::End(TagEnd::TableHead) => self.header = std::mem::take(&mut self.row),
            Event::End(TagEnd::TableRow) => self.rows.push(std::mem::take(&mut self.row)),
            other => self.inline(other),
        }
    }

    /// Inline content and the emphasis it carries.
    fn inline(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::Strong) => self.format.strong = true,
            Event::End(TagEnd::Strong) => self.format.strong = false,
            Event::Start(Tag::Emphasis) => self.format.emphasis = true,
            Event::End(TagEnd::Emphasis) => self.format.emphasis = false,
            Event::Start(Tag::Strikethrough) => self.format.strikethrough = true,
            Event::End(TagEnd::Strikethrough) => self.format.strikethrough = false,
            Event::Code(text) => {
                let format = Format {
                    code: true,
                    ..self.format
                };
                self.push(&text, format);
            }
            Event::Text(text) | Event::InlineHtml(text) => self.push(&text, self.format),
            Event::SoftBreak => self.push(" ", self.format),
            _ => {}
        }
    }

    fn push(&mut self, text: &str, format: Format) {
        self.cell.spans.push((text.to_string(), format));
    }
}

/// Widths that fit `available`, or `None` when the table should stack instead.
///
/// Columns shrink proportionally, but a column pinned at its longest-word width
/// must not push the total back over the available width, so the floor is
/// applied by water-filling: pinned columns leave the pool and the remaining
/// space is rescaled across the rest until nothing else hits its floor.
fn fit(needs: &[Need], available: f32, spacing: f32) -> Option<Vec<f32>> {
    if needs.is_empty() {
        return Some(Vec::new());
    }
    let target = available - spacing * needs.len().saturating_sub(1) as f32;
    let floors: Vec<f32> = needs
        .iter()
        .map(|need| need.word.min(MAX_WORD_MIN))
        .collect();
    if floors.iter().sum::<f32>() > target {
        return None;
    }
    let intrinsic: Vec<f32> = needs.iter().map(|need| need.intrinsic).collect();
    if intrinsic.iter().sum::<f32>() <= target {
        return Some(intrinsic);
    }
    Some(water_fill(&intrinsic, &floors, target))
}

fn water_fill(intrinsic: &[f32], floors: &[f32], target: f32) -> Vec<f32> {
    let mut widths = intrinsic.to_vec();
    let mut pinned = vec![false; intrinsic.len()];
    loop {
        let used: f32 = widths
            .iter()
            .zip(&pinned)
            .filter(|(_, pin)| **pin)
            .map(|(width, _)| *width)
            .sum();
        let free: f32 = intrinsic
            .iter()
            .zip(&pinned)
            .filter(|(_, pin)| !**pin)
            .map(|(width, _)| *width)
            .sum();
        if free <= 0.0 {
            break;
        }
        let scale = (target - used) / free;
        let mut changed = false;
        for (index, width) in widths.iter_mut().enumerate() {
            if !pinned[index] && intrinsic[index] * scale < floors[index] {
                *width = floors[index];
                pinned[index] = true;
                changed = true;
            }
        }
        if !changed {
            for (index, width) in intrinsic.iter().enumerate() {
                if !pinned[index] {
                    widths[index] = width * scale;
                }
            }
            break;
        }
    }
    widths
}

fn measure(ui: &Ui, text: &str) -> f32 {
    let font = ui
        .style()
        .text_styles
        .get(&TextStyle::Body)
        .cloned()
        .unwrap_or_default();
    ui.painter()
        .layout_no_wrap(text.to_string(), font, ui.visuals().text_color())
        .size()
        .x
        + 2.0 * CELL_PAD_X
}

fn needs(ui: &Ui, table: &Table) -> Vec<Need> {
    let mut needs = vec![Need::default(); table.header.len()];
    for cells in std::iter::once(&table.header).chain(table.rows.iter()) {
        for (index, cell) in cells.iter().enumerate() {
            let plain = cell.plain();
            let Some(need) = needs.get_mut(index) else {
                continue;
            };
            need.intrinsic = need.intrinsic.max(measure(ui, &plain));
            for word in plain.split_whitespace() {
                need.word = need.word.max(measure(ui, word));
            }
        }
    }
    needs
}

fn plan(ui: &Ui, table: &Table) -> Plan {
    match fit(
        &needs(ui, table),
        ui.available_width(),
        ui.spacing().item_spacing.x,
    ) {
        Some(widths) => Plan::Columns(widths),
        None => Plan::Stacked,
    }
}

pub fn ui(ui: &mut Ui, table: &Table) {
    let spacing = ui.spacing().item_spacing.x;
    match plan(ui, table) {
        Plan::Columns(widths) => columns(ui, table, &widths, spacing),
        Plan::Stacked => stacked(ui, table),
    }
}

fn columns(ui: &mut Ui, table: &Table, widths: &[f32], spacing: f32) {
    let columns = Columns { widths, spacing };
    header_row(ui, table, &columns);
    for (index, row) in table.rows.iter().enumerate() {
        body_row(ui, row, &columns, index % 2 == 1);
    }
}

fn header_row(ui: &mut Ui, table: &Table, columns: &Columns<'_>) {
    let strong = ui.visuals().strong_text_color();
    let band = ui.painter().add(Shape::Noop);
    let response = columns.row(ui, |ui, index| {
        let text = table
            .header
            .get(index)
            .map_or_else(String::new, |cell| cell.plain().trim().to_string());
        ui.add(egui::Label::new(RichText::new(text).color(strong)).wrap());
    });
    let rect = response.rect;
    let visuals = ui.visuals();
    let fill = visuals.faint_bg_color;
    let rule = visuals.widgets.noninteractive.bg_stroke.color;
    ui.painter().set(band, Shape::rect_filled(rect, 0, fill));
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), egui::Stroke::new(1.0, rule));
}

fn body_row(ui: &mut Ui, cells: &[Cell], columns: &Columns<'_>, striped: bool) {
    let band = striped.then(|| {
        let fill = ui.visuals().faint_bg_color;
        (ui.painter().add(Shape::Noop), fill)
    });
    let response = columns.row(ui, |ui, index| {
        if let Some(cell) = cells.get(index) {
            cell.show(ui, false);
        }
    });
    if let Some((index, fill)) = band {
        ui.painter()
            .set(index, Shape::rect_filled(response.rect, 0, fill));
    }
}

/// Fixed column geometry shared by every row, so headers and body cells line up.
struct Columns<'a> {
    widths: &'a [f32],
    spacing: f32,
}

impl Columns<'_> {
    /// Lay out exactly one framed cell per width.
    fn row(&self, ui: &mut Ui, mut contents: impl FnMut(&mut Ui, usize)) -> egui::Response {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = self.spacing;
            for (index, width) in self.widths.iter().enumerate() {
                let inner = (width - 2.0 * CELL_PAD_X).max(1.0);
                fixed_box(ui, *width, |ui| {
                    fixed_box(ui, inner, |ui| contents(ui, index));
                });
            }
        })
        .response
    }
}

/// A box pinned to `width` with vertical cell padding, so rows share a grid.
fn fixed_box(ui: &mut Ui, width: f32, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::LEFT), |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.add_space(CELL_PAD_Y);
        contents(ui);
        ui.add_space(CELL_PAD_Y);
    });
}

/// One labeled block per row, for narrower widths where columns cannot fit
/// readably. Every cell carries its own header label, including the first, so
/// no column heading is lost when the grid collapses.
fn stacked(ui: &mut Ui, table: &Table) {
    for (index, row) in table.rows.iter().enumerate() {
        let fill = if index % 2 == 1 {
            ui.visuals().faint_bg_color
        } else {
            egui::Color32::TRANSPARENT
        };
        Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin {
                left: CELL_PAD_X as i8,
                right: CELL_PAD_X as i8,
                top: CELL_PAD_Y as i8,
                bottom: CELL_PAD_Y as i8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (column, cell) in row.iter().enumerate() {
                    if cell.is_empty() {
                        continue;
                    }
                    stacked_cell(ui, table, column, cell);
                }
            });
    }
}

fn stacked_cell(ui: &mut Ui, table: &Table, column: usize, cell: &Cell) {
    let muted = ui.visuals().weak_text_color();
    if let Some(label) = table.header.get(column).map(Cell::plain) {
        if !label.trim().is_empty() {
            ui.add(egui::Label::new(RichText::new(label.trim()).small().color(muted)).wrap());
        }
    }
    cell.show(ui, column == 0);
}

#[cfg(test)]
mod tests;
