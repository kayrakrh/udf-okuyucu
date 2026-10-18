//! Sistem yazı tipi bulma, ölçü (units-per-em) ve glif dış çizgileri (vektör).

use std::collections::HashMap;

use owned_ttf_parser::{AsFaceRef, OwnedFace};
use tiny_skia::{Path, PathBuilder};
use ttf_parser::{GlyphId, OutlineBuilder};

struct FaceData {
    face: OwnedFace,
    upem: f32,
}

pub struct Fonts {
    db: fontdb::Database,
    faces: Vec<FaceData>,
    by_id: HashMap<fontdb::ID, usize>,
    by_key: HashMap<(String, bool, bool), Option<usize>>,
    paths: HashMap<(usize, u16), Option<Path>>,
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

fn gid(face: &ttf_parser::Face<'_>, c: char) -> GlyphId {
    let c = if c == '\u{a0}' { ' ' } else { c };
    face.glyph_index(c).unwrap_or(GlyphId(0))
}

struct Outline(PathBuilder);

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

fn build_path(face: &ttf_parser::Face<'_>, g: GlyphId) -> Option<Path> {
    let mut b = Outline(PathBuilder::new());
    face.outline_glyph(g, &mut b)?;
    b.0.finish()
}

impl Fonts {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Fonts { db, faces: Vec::new(), by_id: HashMap::new(), by_key: HashMap::new(), paths: HashMap::new() }
    }

    pub fn resolve(&mut self, family: &str, bold: bool, italic: bool) -> Option<usize> {
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
        let (data, index) = self.db.with_face_data(id, |d, i| (d.to_vec(), i))?;
        let face = OwnedFace::from_vec(data, index).ok()?;
        let upem = face.as_face_ref().units_per_em() as f32;
        self.faces.push(FaceData { face, upem });
        let i = self.faces.len() - 1;
        self.by_id.insert(id, i);
        Some(i)
    }

    pub fn upem(&self, idx: usize) -> f32 {
        self.faces[idx].upem
    }

    /// Bir karakterin ilerlemesi (pt). Zoom'dan bağımsızdır: kesirli, yuvarlamasız.
    pub fn advance(&self, idx: usize, c: char, size: f32) -> f32 {
        let fd = &self.faces[idx];
        let f = fd.face.as_face_ref();
        f.glyph_hor_advance(gid(f, c)).unwrap_or(0) as f32 * size / fd.upem
    }

    pub fn width(&self, font: Option<usize>, text: &str, size: f32) -> f32 {
        match font {
            Some(i) => text.chars().map(|c| self.advance(i, c, size)).sum(),
            None => text.chars().count() as f32 * size * 0.5,
        }
    }

    /// (ascent, descent+satır aralığı) pt cinsinden.
    pub fn vmetrics(&self, font: Option<usize>, size: f32) -> (f32, f32) {
        match font {
            Some(i) => {
                let fd = &self.faces[i];
                let f = fd.face.as_face_ref();
                let k = size / fd.upem;
                (f.ascender() as f32 * k, (-(f.descender() as f32) + f.line_gap() as f32) * k)
            }
            None => (size * 0.9, size * 0.25),
        }
    }

    /// Glifin dış çizgisi, yazı tipi birimlerinde, y yukarı. Yoksa (boşluk vb.) `None`.
    pub fn path(&mut self, idx: usize, c: char) -> Option<&Path> {
        let g = gid(self.faces[idx].face.as_face_ref(), c);
        let key = (idx, g.0);
        if !self.paths.contains_key(&key) {
            let p = build_path(self.faces[idx].face.as_face_ref(), g);
            self.paths.insert(key, p);
        }
        self.paths.get(&key).and_then(|o| o.as_ref())
    }
}
