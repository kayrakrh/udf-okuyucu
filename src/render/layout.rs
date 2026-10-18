//! Satır kırma, hizalama, sekme durakları, tablo ve sayfalama. Birim: pt.
//! Hiçbir yerde zoom/cihaz çözünürlüğü bilinmez; bu yüzden zoom değişince metin yeniden akmaz.

use udflib::{Align, Block, Document, Paragraph, Rgba, TabStop, Table};

use super::{Layout, Op, Renderer};

/// Bölünemez yerleşim birimi: bir metin satırı ya da bir tablo satırı.
struct Line {
    height: f32,
    ops: Vec<Op>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Word,
    Space,
    Tab,
    Break,
}

struct Tok {
    kind: Kind,
    text: String,
    run: usize,
    font: Option<usize>,
    w: f32,
}

struct Item {
    tok: usize,
    x: f32,
    w: f32,
}

const CELL_PAD: f32 = 2.0;
const DEFAULT_TAB: f32 = 36.0;

fn next_tab(tabs: &[TabStop], pen: f32) -> f32 {
    let custom = tabs.iter().map(|t| t.pos).filter(|&p| p > pen + 0.01).fold(f32::INFINITY, f32::min);
    if custom.is_finite() {
        custom
    } else {
        ((pen / DEFAULT_TAB).floor() + 1.0) * DEFAULT_TAB
    }
}

fn class(c: char) -> u8 {
    match c {
        '\n' => 3,
        '\t' => 2,
        c if c.is_whitespace() => 1,
        _ => 0,
    }
}

impl Renderer {
    pub fn layout(&mut self, doc: &Document) -> Layout {
        let (pw, ph) = doc.page.size_pt();
        let m = &doc.page;
        let width = pw - m.left_margin - m.right_margin;
        let lines = self.layout_blocks(&doc.blocks, width);

        let top = m.top_margin;
        let bottom = ph - m.bottom_margin;
        let mut pages: Vec<Vec<Op>> = vec![Vec::new()];
        let mut y = top;
        for line in lines {
            if y + line.height > bottom && y > top + 0.01 {
                pages.push(Vec::new());
                y = top;
            }
            let page = pages.last_mut().unwrap();
            for mut op in line.ops {
                op.shift(m.left_margin, y);
                page.push(op);
            }
            y += line.height;
        }
        Layout { page_w: pw, page_h: ph, pages }
    }

    fn layout_blocks(&mut self, blocks: &[Block], width: f32) -> Vec<Line> {
        let mut out = Vec::new();
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.extend(self.layout_paragraph(p, width)),
                Block::Table(t) => out.extend(self.layout_table(t, width)),
            }
        }
        out
    }

    fn layout_table(&mut self, t: &Table, width: f32) -> Vec<Line> {
        let mut out = Vec::new();
        for row in &t.rows {
            let n = row.len().max(1);
            let cw = width / n as f32;
            let mut cells: Vec<Vec<Line>> = Vec::new();
            let mut row_h: f32 = 0.0;
            for cell in row {
                let lines = self.layout_blocks(&cell.blocks, cw - 2.0 * CELL_PAD);
                row_h = row_h.max(lines.iter().map(|l| l.height).sum());
                cells.push(lines);
            }
            let height = row_h + 2.0 * CELL_PAD;
            let mut ops = Vec::new();
            for (i, lines) in cells.into_iter().enumerate() {
                let x0 = i as f32 * cw;
                let mut y = CELL_PAD;
                for line in lines {
                    for mut op in line.ops {
                        op.shift(x0 + CELL_PAD, y);
                        ops.push(op);
                    }
                    y += line.height;
                }
                if t.border != "borderNone" {
                    let c = Rgba::BLACK;
                    let th = 0.5;
                    ops.push(Op::Rect { x: x0, y: 0.0, w: cw, h: th, color: c });
                    ops.push(Op::Rect { x: x0, y: height - th, w: cw, h: th, color: c });
                    ops.push(Op::Rect { x: x0, y: 0.0, w: th, h: height, color: c });
                    ops.push(Op::Rect { x: x0 + cw - th, y: 0.0, w: th, h: height, color: c });
                }
            }
            out.push(Line { height, ops });
        }
        out
    }

    fn tokenize(&mut self, p: &Paragraph) -> Vec<Tok> {
        let mut out = Vec::new();
        for (ri, run) in p.runs.iter().enumerate() {
            let font = self.fonts.resolve(&run.style.family, run.style.bold, run.style.italic);
            let size = run.style.size;
            let mut pieces: Vec<(u8, String)> = Vec::new();
            for c in run.text.chars() {
                let k = class(c);
                match pieces.last_mut() {
                    Some((pk, s)) if *pk == k && k < 2 => s.push(c),
                    _ => pieces.push((k, c.to_string())),
                }
            }
            for (k, s) in pieces {
                let kind = match k {
                    0 => Kind::Word,
                    1 => Kind::Space,
                    2 => Kind::Tab,
                    _ => Kind::Break,
                };
                let w = if matches!(kind, Kind::Word | Kind::Space) { self.fonts.width(font, &s, size) } else { 0.0 };
                out.push(Tok { kind, text: s, run: ri, font, w });
            }
        }
        out
    }

    fn layout_paragraph(&mut self, p: &Paragraph, width: f32) -> Vec<Line> {
        let ps = &p.style;
        let toks = self.tokenize(p);
        let limit = width - ps.right_indent;

        // 1) Açgözlü satır kırma
        let mut raw: Vec<(Vec<Item>, f32, bool)> = Vec::new(); // (öğeler, başlangıç x, kaydırmayla mı bitti)
        let mut origin = ps.left_indent + ps.first_line_indent;
        let mut items: Vec<Item> = Vec::new();
        let mut pen = origin;
        let mut has_word = false;
        for (ti, t) in toks.iter().enumerate() {
            match t.kind {
                Kind::Break => {
                    raw.push((std::mem::take(&mut items), origin, false));
                    origin = ps.left_indent;
                    pen = origin;
                    has_word = false;
                }
                Kind::Space => {
                    items.push(Item { tok: ti, x: pen, w: t.w });
                    pen += t.w;
                }
                Kind::Tab => {
                    let stop = next_tab(&ps.tabs, pen);
                    items.push(Item { tok: ti, x: pen, w: stop - pen });
                    pen = stop;
                }
                Kind::Word => {
                    if has_word && pen + t.w > limit + 0.01 {
                        while matches!(items.last().map(|i| toks[i.tok].kind), Some(Kind::Space)) {
                            items.pop();
                        }
                        raw.push((std::mem::take(&mut items), origin, true));
                        origin = ps.left_indent;
                        pen = origin;
                        has_word = false;
                    }
                    items.push(Item { tok: ti, x: pen, w: t.w });
                    pen += t.w;
                    has_word = true;
                }
            }
        }
        raw.push((items, origin, false));

        // 2) Satırları oluştur
        let mut lines: Vec<Line> = Vec::new();
        for (items, origin, wrapped) in raw {
            let (mut asc, mut desc) = (0.0f32, 0.0f32);
            for it in &items {
                let t = &toks[it.tok];
                let (a, d) = self.fonts.vmetrics(t.font, p.runs[t.run].style.size);
                asc = asc.max(a);
                desc = desc.max(d);
            }
            if items.is_empty() {
                let (size, font) = match p.runs.first() {
                    Some(r) => (r.style.size, self.fonts.resolve(&r.style.family, r.style.bold, r.style.italic)),
                    None => (12.0, self.fonts.resolve("Times New Roman", false, false)),
                };
                let (a, d) = self.fonts.vmetrics(font, size);
                asc = a;
                desc = d;
            }

            let last_end = items.last().map(|i| i.x + i.w).unwrap_or(origin);
            let natural = last_end - origin;
            let mut dx = match ps.align {
                Align::Center => ((limit - origin) - natural) / 2.0,
                Align::Right => (limit - origin) - natural,
                _ => 0.0,
            };
            if dx < 0.0 {
                dx = 0.0;
            }
            let mut extra = 0.0;
            if ps.align == Align::Justify && wrapped {
                let spaces = items.iter().filter(|i| toks[i.tok].kind == Kind::Space).count();
                if spaces > 0 {
                    extra = (limit - last_end).max(0.0) / spaces as f32;
                }
            }

            let mut ops = Vec::new();
            let mut shift = 0.0;
            for it in &items {
                let t = &toks[it.tok];
                let run = &p.runs[t.run];
                let x = it.x + dx + shift;
                let w = it.w + if t.kind == Kind::Space { extra } else { 0.0 };
                if let Some(bg) = run.style.background {
                    ops.push(Op::Rect { x, y: 0.0, w, h: asc + desc, color: bg });
                }
                if t.kind == Kind::Word {
                    ops.push(Op::Text { x, baseline: asc, text: t.text.clone(), style: run.style.clone() });
                }
                if run.style.underline {
                    let size = run.style.size;
                    ops.push(Op::Rect {
                        x,
                        y: asc + size * 0.12,
                        w,
                        h: (size / 18.0).max(0.5),
                        color: run.style.foreground,
                    });
                }
                if t.kind == Kind::Space {
                    shift += extra;
                }
            }
            lines.push(Line { height: asc + desc, ops });
        }

        // 3) Paragraf öncesi/sonrası boşluk
        if let Some(first) = lines.first_mut() {
            for op in &mut first.ops {
                op.shift(0.0, ps.space_above);
            }
            first.height += ps.space_above;
        }
        if let Some(last) = lines.last_mut() {
            last.height += ps.space_below;
        }
        lines
    }
}
