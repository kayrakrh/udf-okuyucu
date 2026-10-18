//! Belge okuma: `udfdump` ikilisini bulur ve `udfdump --json` çıktısını ayrıştırır.
//! `.udf` dosyasının kendisini bu uygulama okumaz.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Sırayla: `UDFDUMP` ortam değişkeni (geliştirme için), `/usr/bin/udfdump`, `/usr/local/bin/udfdump`.
pub fn find_udfdump() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("UDFDUMP") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    ["/usr/bin/udfdump", "/usr/local/bin/udfdump"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

pub fn read(path: &Path) -> Result<udflib::Document, String> {
    let bin = find_udfdump().ok_or_else(|| {
        "udfdump bulunamadı. /usr/bin/udfdump kurulu olmalı (udflib klasöründe: make && sudo make install)"
            .to_string()
    })?;
    let out = Command::new(&bin)
        .arg("--json")
        .arg("--")
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{} çalıştırılamadı: {e}", bin.display()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("udfdump hata verdi: {}", err.trim()));
    }
    let text = std::str::from_utf8(&out.stdout).map_err(|e| format!("udfdump çıktısı UTF-8 değil: {e}"))?;
    udflib::from_json(text).map_err(|e| e.to_string())
}
