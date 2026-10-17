//! Kullanım: cargo run -p udf-core --bin udfdump -- dosya.udf
use udf_core::{Block, Document};

fn dump_blocks(blocks: &[Block], depth: usize) {
    let pad = "  ".repeat(depth);
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                println!("{pad}¶ [{:?}] {:?}", p.style.align, p.text());
                for r in &p.runs {
                    if r.text.is_empty() {
                        continue;
                    }
                    let tag = r.field.as_deref().map(|f| format!(" field={f}")).unwrap_or_default();
                    println!(
                        "{pad}    {:?} {} {}pt{}{}{}{}",
                        r.text,
                        r.style.family,
                        r.style.size,
                        if r.style.bold { " B" } else { "" },
                        if r.style.italic { " I" } else { "" },
                        if r.style.underline { " U" } else { "" },
                        tag
                    );
                }
            }
            Block::Table(t) => {
                println!("{pad}▦ tablo border={} satır={}", t.border, t.rows.len());
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, cell) in row.iter().enumerate() {
                        println!("{pad}  hücre[{ri}][{ci}]");
                        dump_blocks(&cell.blocks, depth + 2);
                    }
                }
            }
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("kullanım: udfdump <dosya.udf>");
    let doc: Document = match udf_core::open(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("hata: {e}");
            std::process::exit(1);
        }
    };
    println!("format_id : {}", doc.format_id);
    println!("sayfa     : {:?}", doc.page);
    println!("özellikler: {:?}", doc.properties);
    println!("imza      : {} bayt", doc.signature.as_ref().map_or(0, Vec::len));
    println!("webID     : {:?}", doc.web_id);
    println!("---");
    dump_blocks(&doc.blocks, 0);
    if !doc.warnings.is_empty() {
        println!("--- uyarılar");
        for w in &doc.warnings {
            println!("! {w}");
        }
    }
}
