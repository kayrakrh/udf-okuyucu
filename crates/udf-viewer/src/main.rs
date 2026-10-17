use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use udf_render::{Bitmap, Renderer};

slint::include_modules!();

fn to_image(b: &Bitmap) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&b.rgba, b.width, b.height);
    Image::from_rgba8(buf)
}

fn load(win: &MainWindow, renderer: &mut Renderer, path: &Path) -> Result<(), String> {
    let doc = udf_core::open(path).map_err(|e| e.to_string())?;
    for w in &doc.warnings {
        eprintln!("udf uyarı: {w}");
    }
    let layout = renderer.layout(&doc);
    let n = layout.pages.len();
    let pages: Vec<Image> = (0..n).map(|i| to_image(&renderer.render_page(&layout, i, 1.8))).collect();
    let thumbs: Vec<Image> = (0..n).map(|i| to_image(&renderer.render_page(&layout, i, 0.35))).collect();

    win.set_aspect(layout.page_h / layout.page_w);
    win.set_pages(ModelRc::new(VecModel::from(pages)));
    win.set_thumbs(ModelRc::new(VecModel::from(thumbs)));
    win.set_current(0);
    win.set_doc_title(
        path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default().into(),
    );
    win.set_status(format!("{n} sayfa").into());
    Ok(())
}

fn main() -> Result<(), slint::PlatformError> {
    let win = MainWindow::new()?;
    let renderer = Rc::new(RefCell::new(Renderer::new()));

    if let Some(p) = std::env::args().nth(1) {
        if let Err(e) = load(&win, &mut renderer.borrow_mut(), Path::new(&p)) {
            win.set_status(format!("Hata: {e}").into());
        }
    }

    win.on_open_file({
        let weak = win.as_weak();
        let renderer = renderer.clone();
        move || {
            let Some(win) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("UYAP belgesi", &["udf"]).pick_file() else {
                return;
            };
            if let Err(e) = load(&win, &mut renderer.borrow_mut(), &path) {
                win.set_status(format!("Hata: {e}").into());
            }
        }
    });

    win.run()
}
