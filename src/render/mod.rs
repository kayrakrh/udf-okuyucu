//! Yerleşim (pt, zoom'dan bağımsız) + vektör rasterleştirme (istenen ölçekte).

mod fonts;
mod layout;

use fonts::Fonts;
pub use tiny_skia::Pixmap;
use tiny_skia::{Color, FillRule, Paint, Rect as SkRect, Transform};
use udflib::{CharStyle, Rgba};

#[derive(Clone, Debug)]
pub enum Op {
    Text { x: f32, baseline: f32, text: String, style: CharStyle },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgba },
}

impl Op {
    pub(crate) fn shift(&mut self, dx: f32, dy: f32) {
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

pub struct Renderer {
    fonts: Fonts,
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, c.3);
    p.anti_alias = true;
    p
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { fonts: Fonts::new() }
    }

    /// `scale`: piksel/pt. Opak beyaz zeminli pixmap döner (premultiplied == düz).
    pub fn render_page(&mut self, layout: &Layout, index: usize, scale: f32) -> Pixmap {
        let w = (layout.page_w * scale).round().max(1.0) as u32;
        let h = (layout.page_h * scale).round().max(1.0) as u32;
        let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
        pm.fill(Color::WHITE);
        let Some(ops) = layout.pages.get(index) else {
            return pm;
        };
        let hair = 1.0 / scale; // en az 1 cihaz pikseli: ince çizgiler düşük zoom'da kaybolmasın
        for op in ops {
            match op {
                Op::Rect { x, y, w, h, color } => {
                    if let Some(r) = SkRect::from_xywh(x * scale, y * scale, w.max(hair) * scale, h.max(hair) * scale) {
                        pm.fill_rect(r, &paint(*color), Transform::identity(), None);
                    }
                }
                Op::Text { x, baseline, text, style } => self.draw_text(&mut pm, *x, *baseline, text, style, scale),
            }
        }
        pm
    }

    fn draw_text(&mut self, pm: &mut Pixmap, x: f32, baseline: f32, text: &str, st: &CharStyle, scale: f32) {
        let Some(fi) = self.fonts.resolve(&st.family, st.bold, st.italic) else {
            return;
        };
        let k = st.size * scale / self.fonts.upem(fi);
        let paint = paint(st.foreground);
        let mut pen = x; // pt; ilerleme yerleşimdekiyle aynı fonksiyondan, yuvarlama yok
        for c in text.chars() {
            let adv = self.fonts.advance(fi, c, st.size);
            if let Some(path) = self.fonts.path(fi, c) {
                let ts = Transform::from_row(k, 0.0, 0.0, -k, pen * scale, baseline * scale);
                pm.fill_path(path, &paint, FillRule::Winding, ts, None);
            }
            pen += adv;
        }
    }
}
