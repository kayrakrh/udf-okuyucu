//! Yerleşim (satır kırma, hizalama, tablo, sayfalama) ve CPU ile rasterleştirme.
//!
//! Tüm yerleşim pt cinsindendir; çizimde `scale` (piksel/pt) ile çarpılır.
//! Yazı tipleri sistemden `fontdb` ile bulunur. "Times New Roman" yoksa metrik uyumlu
//! "Liberation Serif" (Arch: `ttf-liberation`) kullanılır, böylece satır sonları
//! orijinaline yakın çıkar.

use std::collections::HashMap;

use udf_core::{Align, Block, CharStyle, Document, Paragraph, Rgba, Table, TabStop};

// ───────────────────────── Genel tipler ─────────────────────────

pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// RGBA8, düz (premultiplied değil), opak beyaz zemin.
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum Op {
    Text { x: f32, baseline: f32, text: String, style: CharStyle },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgba },
}

impl Op {
    fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Op::Text { x, baseline, .. } => {
                *x += dx;
                *baseline += dy;
            }
            Op::Rect { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
        }
    }
}

pub struct Layout {
    pub page_w: f32,
    pub page_h: f32,
    pub pages: Vec<Vec<Op>>,
}

/// Bölünemez yerleşim birimi: bir metin satırı ya da bir tablo satırı.
struct Line {
    height: f32,
    ops: Vec<Op>,
}

// ───────────────────────── Yazı tipleri ─────────────────────────

struct Fonts {
    db: fontdb::Database,
    faces: Vec<fontdue::Font>,
    by_id: HashMap<fontdb::ID, usize>,
    by_key: HashMap<(String, bool, bool), Option<usize>>,
    adv: HashMap<(usize, char, u32), f32>,
}

fn candidates(family: &str) -> Vec<String> {
    let lower = family.to_lowercase();
    let extra: &[&str] = if lower.contains("times") || lower == "serif" {
        &["Liberation Serif", "Tinos", "Nimbus Roman", "FreeSerif", "DejaVu Serif"]
    } else if ["arial", "helvetica", "calibri", "verdana", "tahoma"].iter().any(|k| lower.contains(k)) {
        &["Liberation Sans", "Arimo", "Nimbus Sans", "FreeSans", "DejaVu Sans"]
    } else if lower.contains("courier") || lower.contains("mono") {
        &["Liberation Mono", "Cousine", "DejaVu Sans Mono"]
    } else {
        &["Liberation Serif", "DejaVu Serif", "Noto Serif", "Liberation Sans", "DejaVu Sans", "Noto Sans"]
    };
    let mut v = vec![family.to_owned()];
    v.extend(extra.iter().map(|s| s.to_string()));
    v
}

impl Fonts {
    fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Fonts { db, faces: Vec::new(), by_id: HashMap::new(), by_key: HashMap::new(), adv: HashMap::new() }
    }

    fn resolve(&mut self, family: &str, bold: bool, italic: bool) -> Option<usize> {
        let key = (family.to_lowercase(), bold, italic);
        if let Some(v) = self.by_key.get(&key) {
            return *v;
        }
        let r = self.load(family, bold, italic);
        self.by_key.insert(key, r);
        r
    }

    fn load(&mut self, family: &str, bold: bool, italic: bool) -> Option<usize> {
        let names = candidates(family);
        let fams: Vec<fontdb::Family> = names.iter().map(|n| fontdb::Family::Name(n.as_str())).collect();
        let q = fontdb::Query {
            families: &fams,
            weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            stretch: fontdb::Stretch::Normal,
            style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        let id = self.db.query(&q)?;
        if let Some(&i) = self.by_id.get(&id) {
            return Some(i);
        }
        let font = self.db.with_face_data(id, |data, index| {
            fontdue::Font::from_bytes(
                data,
                fontdue::FontSettings { collection_index: index, ..Default::default() },
            )
            .ok()
        })??;
        self.faces.push(font);
        let i = self.faces.len() - 1;
        self.by_id.insert(id, i);
        Some(i)
    }

    /// (ascent, descent) pt cinsinden, ikisi de pozitif.
    fn vmetrics(&self, font: Option<usize>, size: f32) -> (f32, f32) {
        match font.and_then(|i| self.faces.get(i)).and_then(|f| f.horizontal_line_metrics(size)) {
            Some(m) => (m.ascent, -m.descent),
            None => (size * 0.9, size * 0.25),
        }
    }

    fn width(&mut self, font: Option<usize>, text: &str, size: f32) -> f32 {
        let Some(i) = font else {
            return text.chars().count() as f32 * size * 0.5;
        };
        let mut w = 0.0;
        for c in text.chars() {
            let key = (i, c, size.to_bits());
            let a = *self
                .adv
                .entry(key)
                .or_insert_with(|| self.faces[i].metrics(c, size).advance_width);
            w += a;
        }
        w
    }
}

// ───────────────────────── Yerleşim ─────────────────────────

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

pub struct Renderer {
    fonts: Fonts,
    glyphs: HashMap<(usize, char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { fonts: Fonts::new(), glyphs: HashMap::new() }
    }

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
            let mut buf = String::new();
            let mut cls = u8::MAX;
            let mut flush = |this: &mut Self, buf: &mut String, cls: u8| {
                if buf.is_empty() {
                    return;
                }
                let kind = match cls {
                    0 => Kind::Word,
                    1 => Kind::Space,
                    2 => Kind::Tab,
                    _ => Kind::Break,
                };
                let w = if matches!(kind, Kind::Word | Kind::Space) {
                    this.fonts.width(font, buf, size)
                } else {
                    0.0
                };
                out.push(Tok { kind, text: std::mem::take(buf), run: ri, font, w });
            };
            for c in run.text.chars() {
                let k = class(c);
                if k != cls || k >= 2 {
                    flush(self, &mut buf, cls);
                    cls = k;
                }
                buf.push(c);
            }
            flush(self, &mut buf, cls);
        }
        out
    }

    fn layout_paragraph(&mut self, p: &Paragraph, width: f32) -> Vec<Line> {
        let ps = &p.style;
        let toks = self.tokenize(p);
        let limit = width - ps.right_indent;

        // 1) Greedy satır kırma
        let mut raw: Vec<(Vec<Item>, f32, bool)> = Vec::new(); // (öğeler, başlangıç x, kaydırma ile mi bitti)
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

    // ───────────────────────── Rasterleştirme ─────────────────────────

    pub fn render_page(&mut self, layout: &Layout, index: usize, scale: f32) -> Bitmap {
        let w = (layout.page_w * scale).round().max(1.0) as u32;
        let h = (layout.page_h * scale).round().max(1.0) as u32;
        let mut bm = Bitmap { width: w, height: h, rgba: vec![255; (w * h * 4) as usize] };
        if let Some(ops) = layout.pages.get(index) {
            for op in ops {
                match op {
                    Op::Rect { x, y, w, h, color } => fill_rect(&mut bm, *x, *y, *w, *h, *color, scale),
                    Op::Text { x, baseline, text, style } => {
                        self.draw_text(&mut bm, *x, *baseline, text, style, scale)
                    }
                }
            }
        }
        bm
    }

    fn draw_text(&mut self, bm: &mut Bitmap, x: f32, baseline: f32, text: &str, st: &CharStyle, scale: f32) {
        let Some(fi) = self.fonts.resolve(&st.family, st.bold, st.italic) else {
            return;
        };
        let px = st.size * scale;
        let mut pen = x * scale;
        let base = baseline * scale;
        for ch in text.chars() {
            let key = (fi, ch, px.to_bits());
            if !self.glyphs.contains_key(&key) {
                let g = self.fonts.faces[fi].rasterize(ch, px);
                self.glyphs.insert(key, g);
            }
            let (m, bitmap) = &self.glyphs[&key];
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (base - m.ymin as f32 - m.height as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let cov = bitmap[row * m.width + col];
                    if cov == 0 {
                        continue;
                    }
                    let xx = gx + col as i32;
                    let yy = gy + row as i32;
                    if xx >= 0 && yy >= 0 && (xx as u32) < bm.width && (yy as u32) < bm.height {
                        blend(bm, xx as usize, yy as usize, st.foreground, cov as f32 / 255.0);
                    }
                }
            }
            pen += m.advance_width;
        }
    }
}

fn blend(bm: &mut Bitmap, x: usize, y: usize, c: Rgba, cov: f32) {
    let a = cov * (c.3 as f32 / 255.0);
    if a <= 0.0 {
        return;
    }
    let i = (y * bm.width as usize + x) * 4;
    let src = [c.0, c.1, c.2];
    for k in 0..3 {
        let d = bm.rgba[i + k] as f32;
        bm.rgba[i + k] = (d + (src[k] as f32 - d) * a).round().clamp(0.0, 255.0) as u8;
    }
}

fn fill_rect(bm: &mut Bitmap, x: f32, y: f32, w: f32, h: f32, c: Rgba, scale: f32) {
    let x0 = (x * scale).round().max(0.0) as usize;
    let y0 = (y * scale).round().max(0.0) as usize;
    let mut x1 = ((x + w) * scale).round().max(0.0) as usize;
    let mut y1 = ((y + h) * scale).round().max(0.0) as usize;
    if x1 <= x0 {
        x1 = x0 + 1;
    }
    if y1 <= y0 {
        y1 = y0 + 1;
    }
    for yy in y0..y1.min(bm.height as usize) {
        for xx in x0..x1.min(bm.width as usize) {
            blend(bm, xx, yy, c, 1.0);
        }
    }
}
