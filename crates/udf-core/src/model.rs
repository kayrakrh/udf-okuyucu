//! UDF belge modeli. Sadece veri tutar; yerleşim/çizim başka katmanın işi.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const BLACK: Rgba = Rgba(0, 0, 0, 255);

    /// Java `Color.getRGB()` tarzı işaretli ARGB tamsayısı (ör. -196608).
    pub fn from_java_argb(v: i32) -> Self {
        let u = v as u32;
        Rgba((u >> 16) as u8, (u >> 8) as u8, u as u8, (u >> 24) as u8)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharStyle {
    pub family: String,
    /// punto (pt)
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub foreground: Rgba,
    pub background: Option<Rgba>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabStop {
    pub pos: f32,
    /// Ham değerler (`pos:hizalama:dolgu`); anlamları henüz doğrulanmadı.
    pub align: u8,
    pub leader: u8,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParaStyle {
    pub align: Align,
    pub left_indent: f32,
    pub right_indent: f32,
    pub first_line_indent: f32,
    pub space_above: f32,
    pub space_below: f32,
    pub line_spacing: f32,
    pub tabs: Vec<TabStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// Paragraf sonundaki '\n' atılmıştır; ortadaki '\n' zorunlu satır sonudur.
    pub text: String,
    pub style: CharStyle,
    /// `<field fieldName="...">` ise alan adı (esasNo, kararNo, ...).
    pub field: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub style: ParaStyle,
    /// Boş paragrafta bile en az bir (boş metinli) run bulunur; satır yüksekliği için.
    pub runs: Vec<Run>,
}

impl Paragraph {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Ham değer, ör. "borderNone".
    pub border: String,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageFormat {
    pub media_size_name: u8,
    pub orientation: u8,
    pub left_margin: f32,
    pub right_margin: f32,
    pub top_margin: f32,
    pub bottom_margin: f32,
    pub header_offset: f32,
    pub footer_offset: f32,
}

impl Default for PageFormat {
    fn default() -> Self {
        PageFormat {
            media_size_name: 1,
            orientation: 1,
            left_margin: 42.519684,
            right_margin: 42.519684,
            top_margin: 42.519684,
            bottom_margin: 14.173228,
            header_offset: 15.0,
            footer_offset: 60.0,
        }
    }
}

impl PageFormat {
    /// Sayfa boyutu (pt). VARSAYIM: `mediaSizeName` değeri ne olursa olsun A4 dikey.
    /// Örnekte "1" ve gerçek belge A4 olduğu için şimdilik böyle; başka değerleri gören
    /// örnek dosyalar geldikçe doğrulanacak.
    pub fn size_pt(&self) -> (f32, f32) {
        (595.276, 841.89)
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub format_id: String,
    pub page: PageFormat,
    pub blocks: Vec<Block>,
    /// documentproperties.xml (uyapdogrulamakodu, uyapsicil, ...)
    pub properties: BTreeMap<String, String>,
    /// sign.sgn ham baytları (CMS/PKCS#7 SignedData, DER). Doğrulama henüz yok.
    pub signature: Option<Vec<u8>>,
    pub web_id: Option<String>,
    /// Tanınmayan öğe/öznitelikler; yeni örnek dosyalar için ipucu.
    pub warnings: Vec<String>,
}
