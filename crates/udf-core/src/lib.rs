//! UYAP `.udf` okuyucu.
//!
//! Bir `.udf` dosyası ZIP arşividir:
//! - `content.xml`            : belge (metin + paragraf/tablo yapısı + stiller), zorunlu
//! - `documentproperties.xml` : Java Properties XML (doğrulama kodu, sicil, ...)
//! - `sign.sgn`               : e-imza (DER kodlu CMS SignedData)

mod error;
mod model;
mod parse;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

pub use error::UdfError;
pub use model::*;

pub fn open<P: AsRef<Path>>(path: P) -> Result<Document, UdfError> {
    from_reader(File::open(path)?)
}

pub fn from_bytes(bytes: &[u8]) -> Result<Document, UdfError> {
    from_reader(std::io::Cursor::new(bytes))
}

pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Document, UdfError> {
    let mut zip = zip::ZipArchive::new(reader)?;

    let content = read_entry(&mut zip, "content.xml")?.ok_or(UdfError::Missing("content.xml"))?;
    let content = String::from_utf8(content)
        .map_err(|e| UdfError::Invalid(format!("content.xml UTF-8 değil: {e}")))?;
    let parsed = parse::parse_content(&content)?;

    let properties = match read_entry(&mut zip, "documentproperties.xml")? {
        Some(b) => parse_properties(&String::from_utf8_lossy(&b))?,
        None => BTreeMap::new(),
    };
    let signature = read_entry(&mut zip, "sign.sgn")?;

    Ok(Document {
        format_id: parsed.format_id,
        page: parsed.page,
        blocks: parsed.blocks,
        properties,
        signature,
        web_id: parsed.web_id,
        warnings: parsed.warnings,
    })
}

fn read_entry<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>, UdfError> {
    match zip.by_name(name) {
        Ok(mut f) => {
            let mut buf = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut buf)?;
            Ok(Some(buf))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Java `Properties.storeToXML` biçimi; DOCTYPE içerir, bu yüzden DTD'ye izin verilir
/// (roxmltree dış DTD'yi indirmez).
fn parse_properties(xml: &str) -> Result<BTreeMap<String, String>, UdfError> {
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(xml, opts)?;
    Ok(doc
        .descendants()
        .filter(|n| n.is_element() && n.has_tag_name("entry"))
        .filter_map(|n| Some((n.attribute("key")?.to_owned(), n.text().unwrap_or("").to_owned())))
        .collect())
}
