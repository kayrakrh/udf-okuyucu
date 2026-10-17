//! content.xml ayrıştırıcısı.
//!
//! Format (örnekten gözlenen): `<content><![CDATA[...]]></content>` tüm düz metni tutar;
//! `<elements>` altındaki paragraf/run'lar bu metne `startOffset`/`length` ile bakar.
//! Ofsetler Java `String` indeksidir, yani UTF-16 kod birimi cinsindendir; bu yüzden
//! metni `Vec<u16>` olarak tutup dilimliyoruz (emoji gibi BMP dışı karakterlerde
//! `char` indeksi kayardı).

use std::collections::HashMap;

use roxmltree::Node;

use crate::error::UdfError;
use crate::model::*;

pub(crate) struct Parsed {
    pub format_id: String,
    pub page: PageFormat,
    pub blocks: Vec<Block>,
    pub web_id: Option<String>,
    pub warnings: Vec<String>,
}

struct Ctx {
    text: Vec<u16>,
    styles: HashMap<String, HashMap<String, String>>,
    warnings: Vec<String>,
}

pub(crate) fn parse_content(xml: &str) -> Result<Parsed, UdfError> {
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    if root.tag_name().name() != "template" {
        return Err(UdfError::Invalid(format!(
            "kök öğe <template> olmalıydı, <{}> bulundu",
            root.tag_name().name()
        )));
    }

    let child = |name: &str| root.children().find(|n| n.is_element() && n.has_tag_name(name));

    let text16: Vec<u16> = child("content")
        .and_then(|n| n.text())
        .unwrap_or("")
        .encode_utf16()
        .collect();

    let mut styles = HashMap::new();
    if let Some(styles_node) = child("styles") {
        for s in styles_node.children().filter(|n| n.is_element() && n.has_tag_name("style")) {
            if let Some(name) = s.attribute("name") {
                let attrs = s
                    .attributes()
                    .map(|a| (a.name().to_owned(), a.value().to_owned()))
                    .collect();
                styles.insert(name.to_owned(), attrs);
            }
        }
    }

    let mut ctx = Ctx { text: text16, styles, warnings: Vec::new() };

    let page = child("properties")
        .and_then(|p| p.children().find(|n| n.is_element() && n.has_tag_name("pageFormat")))
        .map(|n| parse_page_format(n))
        .unwrap_or_default();

    let blocks = match child("elements") {
        Some(el) => ctx.parse_blocks(el),
        None => Vec::new(),
    };

    Ok(Parsed {
        format_id: root.attribute("format_id").unwrap_or("").to_owned(),
        page,
        blocks,
        web_id: child("webID").and_then(|n| n.attribute("id")).map(str::to_owned),
        warnings: ctx.warnings,
    })
}

fn parse_page_format(n: Node<'_, '_>) -> PageFormat {
    let d = PageFormat::default();
    let f = |k: &str, def: f32| n.attribute(k).and_then(|v| v.trim().parse().ok()).unwrap_or(def);
    let u = |k: &str, def: u8| n.attribute(k).and_then(|v| v.trim().parse().ok()).unwrap_or(def);
    PageFormat {
        media_size_name: u("mediaSizeName", d.media_size_name),
        orientation: u("paperOrientation", d.orientation),
        left_margin: f("leftMargin", d.left_margin),
        right_margin: f("rightMargin", d.right_margin),
        top_margin: f("topMargin", d.top_margin),
        bottom_margin: f("bottomMargin", d.bottom_margin),
        header_offset: f("headerFOffset", d.header_offset),
        footer_offset: f("footerFOffset", d.footer_offset),
    }
}

impl Ctx {
    /// Stil zincirinde (`resolver` → üst stil → ...) arar; "default" stiline bakmaz.
    fn chain(&self, resolver: Option<&str>, key: &str) -> Option<&String> {
        let mut cur = resolver;
        for _ in 0..16 {
            let name = cur?;
            let def = self.styles.get(name)?;
            if let Some(v) = def.get(key) {
                return Some(v);
            }
            cur = def.get("resolver").map(String::as_str);
        }
        None
    }

    /// Sırayla: her düğümün kendi özniteliği, sonra onun stil zinciri; en sonda "default" stili.
    fn attr(&self, nodes: &[Node<'_, '_>], key: &str) -> Option<String> {
        for n in nodes {
            if let Some(v) = n.attribute(key) {
                return Some(v.to_owned());
            }
            if let Some(v) = self.chain(n.attribute("resolver"), key) {
                return Some(v.clone());
            }
        }
        self.styles.get("default").and_then(|d| d.get(key)).cloned()
    }

    fn f32_attr(&self, nodes: &[Node<'_, '_>], key: &str) -> Option<f32> {
        self.attr(nodes, key)?.trim().parse().ok()
    }

    fn bool_attr(&self, nodes: &[Node<'_, '_>], key: &str) -> bool {
        matches!(self.attr(nodes, key).as_deref(), Some("true"))
    }

    fn color_attr(&self, nodes: &[Node<'_, '_>], key: &str) -> Option<Rgba> {
        let v: i32 = self.attr(nodes, key)?.trim().parse().ok()?;
        Some(Rgba::from_java_argb(v))
    }

    fn slice(&mut self, start: usize, len: usize) -> String {
        let total = self.text.len();
        let end = start.saturating_add(len);
        if end > total {
            self.warnings
                .push(format!("ofset aralığı metni aşıyor: {start}+{len} > {total}"));
        }
        let (s, e) = (start.min(total), end.min(total));
        String::from_utf16_lossy(&self.text[s..e])
    }

    fn parse_blocks(&mut self, parent: Node<'_, '_>) -> Vec<Block> {
        let mut out = Vec::new();
        for child in parent.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "paragraph" => out.push(Block::Paragraph(self.parse_paragraph(child))),
                "table" => out.push(Block::Table(self.parse_table(child))),
                other => self.warnings.push(format!("desteklenmeyen blok öğesi <{other}>")),
            }
        }
        out
    }

    fn parse_table(&mut self, t: Node<'_, '_>) -> Table {
        let mut rows = Vec::new();
        for row in t.children().filter(|n| n.is_element()) {
            if !row.has_tag_name("row") {
                self.warnings
                    .push(format!("tablo içinde desteklenmeyen öğe <{}>", row.tag_name().name()));
                continue;
            }
            let mut cells = Vec::new();
            for cell in row.children().filter(|n| n.is_element()) {
                if !cell.has_tag_name("cell") {
                    self.warnings
                        .push(format!("satır içinde desteklenmeyen öğe <{}>", cell.tag_name().name()));
                    continue;
                }
                cells.push(Cell { blocks: self.parse_blocks(cell) });
            }
            rows.push(cells);
        }
        Table { border: t.attribute("border").unwrap_or("").to_owned(), rows }
    }

    fn parse_paragraph(&mut self, p: Node<'_, '_>) -> Paragraph {
        let pn = [p];
        let style = ParaStyle {
            align: match self.attr(&pn, "Alignment").as_deref() {
                Some("1") => Align::Center,
                Some("2") => Align::Right,
                Some("3") => Align::Justify,
                _ => Align::Left,
            },
            left_indent: self.f32_attr(&pn, "LeftIndent").unwrap_or(0.0),
            right_indent: self.f32_attr(&pn, "RightIndent").unwrap_or(0.0),
            first_line_indent: self.f32_attr(&pn, "FirstLineIndent").unwrap_or(0.0),
            space_above: self.f32_attr(&pn, "SpaceAbove").unwrap_or(0.0),
            space_below: self.f32_attr(&pn, "SpaceBelow").unwrap_or(0.0),
            line_spacing: self.f32_attr(&pn, "LineSpacing").unwrap_or(0.0),
            tabs: parse_tabs(self.attr(&pn, "TabSet").as_deref().unwrap_or("")),
        };

        let mut runs = Vec::new();
        for r in p.children().filter(|n| n.is_element()) {
            let name = r.tag_name().name();
            if !matches!(name, "content" | "field" | "space") {
                self.warnings.push(format!("paragraf içinde desteklenmeyen öğe <{name}>"));
                continue;
            }
            let (Some(start), Some(len)) = (
                r.attribute("startOffset").and_then(|v| v.parse::<usize>().ok()),
                r.attribute("length").and_then(|v| v.parse::<usize>().ok()),
            ) else {
                self.warnings.push(format!("<{name}> startOffset/length eksik"));
                continue;
            };
            let text = self.slice(start, len);
            let nodes = [r, p];
            let cs = CharStyle {
                family: self.attr(&nodes, "family").unwrap_or_else(|| "Times New Roman".into()),
                size: self.f32_attr(&nodes, "size").unwrap_or(12.0),
                bold: self.bool_attr(&nodes, "bold"),
                italic: self.bool_attr(&nodes, "italic"),
                underline: self.bool_attr(&nodes, "underline"),
                foreground: self.color_attr(&nodes, "foreground").unwrap_or(Rgba::BLACK),
                background: self.color_attr(&nodes, "background"),
            };
            runs.push(Run {
                text,
                style: cs,
                field: if name == "field" { r.attribute("fieldName").map(str::to_owned) } else { None },
            });
        }

        // Paragrafın son karakteri paragraf sonu işaretidir ('\n'), görüntülenmez.
        if let Some(last) = runs.last_mut() {
            if last.text.ends_with('\n') {
                last.text.pop();
            }
        }
        Paragraph { style, runs }
    }
}

/// `TabSet="69.0:0:0"` — birden çok durak için ayırıcı doğrulanmadı; ',' ve ';' kabul edilir.
fn parse_tabs(s: &str) -> Vec<TabStop> {
    s.split(|c| c == ',' || c == ';')
        .filter(|part| !part.trim().is_empty())
        .filter_map(|part| {
            let mut it = part.trim().split(':');
            let pos = it.next()?.parse().ok()?;
            let align = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let leader = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            Some(TabStop { pos, align, leader })
        })
        .collect()
}
