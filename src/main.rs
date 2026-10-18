mod backend;
mod render;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, VecModel};

use render::{Layout, Pixmap, Renderer};

slint::include_modules!();

/// Mantıksal piksel / pt (CSS 96 dpi).
const CSS_PX_PER_PT: f32 = 96.0 / 72.0;
const GAP_PT: f32 = 10.0;
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 8.0;
/// Tek sayfa dokusunun en uzun kenarı (piksel); çok yüksek zoom'da bellek patlamasın.
const MAX_TEXTURE: f32 = 4096.0;
const THUMB_W: f32 = 128.0;

struct State {
    renderer: Renderer,
    layout: Option<Rc<Layout>>,
    zoom: f32,
    pages: Rc<VecModel<Image>>,
    /// sayfa -> çizildiği ölçek anahtarı
    rendered: HashMap<usize, u32>,
    pending_fit: bool,
}

type Shared = Rc<RefCell<State>>;

fn to_image(pm: &Pixmap) -> Image {
    // Zemin opak olduğundan premultiplied == düz RGBA.
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(pm.data(), pm.width(), pm.height());
    Image::from_rgba8(buf)
}

/// (sayfa genişliği, sayfa yüksekliği, boşluk) — mantıksal piksel, zoom ile doğrusal.
fn geometry(st: &State) -> (f32, f32, f32) {
    let Some(l) = &st.layout else { return (0.0, 0.0, 0.0) };
    let k = CSS_PX_PER_PT * st.zoom;
    (l.page_w * k, l.page_h * k, GAP_PT * k)
}

fn render_scale(l: &Layout, zoom: f32, dpr: f32) -> f32 {
    (zoom * CSS_PX_PER_PT * dpr).min(MAX_TEXTURE / l.page_w.max(l.page_h))
}

fn apply_geometry(win: &MainWindow, st: &State) {
    let (pw, ph, gap) = geometry(st);
    win.set_page_w(pw);
    win.set_page_h(ph);
    win.set_gap(gap);
    win.set_zoom_text(format!("%{}", (st.zoom * 100.0).round() as i32).into());
}

/// Görünen sayfaları (±1) çizer, uzaktakileri bellekten atar.
/// `force`: ölçeği eskimiş sayfaları da yeniden çiz (zoom durulunca).
fn update_visible(win: &MainWindow, st: &mut State, force: bool) {
    let Some(layout) = st.layout.clone() else { return };
    let n = layout.pages.len();
    if n == 0 {
        return;
    }
    let (_, ph, gap) = geometry(st);
    let step = ph + gap;
    if step <= 0.0 {
        return;
    }
    let top = -win.get_scroll_y();
    let view_h = win.get_view_h();
    let idx = |y: f32| (((y - gap) / step).floor().max(0.0) as usize).min(n - 1);
    let first = idx(top);
    let last = idx(top + view_h);
    win.set_current(idx(top + view_h * 0.35) as i32);

    let dpr = win.window().scale_factor();
    let scale = render_scale(&layout, st.zoom, dpr);
    let key = (scale * 1000.0).round() as u32;

    let lo = first.saturating_sub(1);
    let hi = (last + 1).min(n - 1);
    for i in lo..=hi {
        let stale = match st.rendered.get(&i) {
            None => true,
            Some(k) => force && *k != key,
        };
        if stale {
            let pm = st.renderer.render_page(&layout, i, scale);
            st.pages.set_row_data(i, to_image(&pm));
            st.rendered.insert(i, key);
        }
    }

    let keep_lo = lo.saturating_sub(2);
    let keep_hi = (hi + 2).min(n - 1);
    let drop: Vec<usize> = st.rendered.keys().copied().filter(|i| *i < keep_lo || *i > keep_hi).collect();
    for i in drop {
        st.pages.set_row_data(i, Image::default());
        st.rendered.remove(&i);
    }
}

/// (sx, sy): imlecin görünüm penceresine göre konumu; belgedeki o nokta yerinde kalır.
fn zoom_to(win: &MainWindow, st: &mut State, new: f32, sx: f32, sy: f32) {
    let Some(layout) = st.layout.clone() else { return };
    let new = new.clamp(ZOOM_MIN, ZOOM_MAX);
    let old = st.zoom;
    if (new - old).abs() < 1e-4 {
        return;
    }
    let f = new / old;
    let cx = -win.get_scroll_x() + sx;
    let cy = -win.get_scroll_y() + sy;
    st.zoom = new;
    apply_geometry(win, st);

    let (pw, ph, gap) = geometry(st);
    let (vw, vh) = (win.get_view_w(), win.get_view_h());
    let content_w = (pw + 2.0 * gap).max(vw);
    let content_h = layout.pages.len() as f32 * (ph + gap) + gap;
    let nx = if pw + 2.0 * gap <= vw { 0.0 } else { (sx - cx * f).clamp((vw - content_w).min(0.0), 0.0) };
    let ny = (sy - cy * f).clamp((vh - content_h).min(0.0), 0.0);
    win.set_scroll_x(nx);
    win.set_scroll_y(ny);
    update_visible(win, st, false);
}

fn zoom_center(win: &MainWindow, st: &mut State, new: f32) {
    let (vw, vh) = (win.get_view_w(), win.get_view_h());
    zoom_to(win, st, new, vw / 2.0, vh / 2.0);
}

fn fit_zoom(win: &MainWindow, st: &State, whole_page: bool) -> Option<f32> {
    let l = st.layout.as_ref()?;
    let by_w = (win.get_view_w() - 48.0) / (l.page_w * CSS_PX_PER_PT);
    let z = if whole_page {
        by_w.min((win.get_view_h() - 32.0) / (l.page_h * CSS_PX_PER_PT))
    } else {
        by_w
    };
    Some(z)
}

fn load(win: &MainWindow, st: &mut State, path: &Path) -> Result<(), String> {
    let doc = backend::read(path)?;
    for w in &doc.warnings {
        eprintln!("udflib uyarı: {w}");
    }
    let layout = Rc::new(st.renderer.layout(&doc));
    let n = layout.pages.len();
    let dpr = win.window().scale_factor();
    let ts = THUMB_W * dpr / layout.page_w;
    let thumbs: Vec<Image> = (0..n).map(|i| to_image(&st.renderer.render_page(&layout, i, ts))).collect();

    st.pages = Rc::new(VecModel::from(vec![Image::default(); n]));
    st.rendered.clear();
    st.layout = Some(layout.clone());
    st.zoom = 1.0;
    st.pending_fit = true;

    win.set_pages(ModelRc::from(st.pages.clone()));
    win.set_thumbs(ModelRc::new(VecModel::from(thumbs)));
    win.set_thumb_h(THUMB_W * layout.page_h / layout.page_w);
    win.set_scroll_x(0.0);
    win.set_scroll_y(0.0);
    win.set_doc_title(path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default().into());
    win.set_status(format!("{n} sayfa").into());
    apply_geometry(win, st);
    update_visible(win, st, true);
    Ok(())
}

fn main() -> Result<(), slint::PlatformError> {
    let win = MainWindow::new()?;
    let st: Shared = Rc::new(RefCell::new(State {
        renderer: Renderer::new(),
        layout: None,
        zoom: 1.0,
        pages: Rc::new(VecModel::default()),
        rendered: HashMap::new(),
        pending_fit: false,
    }));
    let timer = Rc::new(Timer::default());

    // Zoom durulunca (~140 ms) görünen sayfaları yeni ölçekte keskin çiz.
    let schedule: Rc<dyn Fn()> = {
        let (weak, st, timer) = (win.as_weak(), st.clone(), timer.clone());
        Rc::new(move || {
            let (weak, st) = (weak.clone(), st.clone());
            timer.start(TimerMode::SingleShot, Duration::from_millis(140), move || {
                if let Some(w) = weak.upgrade() {
                    if let Ok(mut s) = st.try_borrow_mut() {
                        update_visible(&w, &mut s, true);
                    }
                }
            });
        })
    };

    macro_rules! cb {
        (|$w:ident, $s:ident| $body:block) => {{
            let (weak, st, sch) = (win.as_weak(), st.clone(), schedule.clone());
            move || {
                let Some($w) = weak.upgrade() else { return };
                {
                    let Ok(mut $s) = st.try_borrow_mut() else { return };
                    $body
                }
                sch();
            }
        }};
    }

    win.on_zoom_in(cb!(|w, s| { let z = s.zoom * 1.25; zoom_center(&w, &mut s, z); }));
    win.on_zoom_out(cb!(|w, s| { let z = s.zoom / 1.25; zoom_center(&w, &mut s, z); }));
    win.on_zoom_reset(cb!(|w, s| { zoom_center(&w, &mut s, 1.0); }));
    win.on_fit_width(cb!(|w, s| {
        if let Some(z) = fit_zoom(&w, &s, false) { zoom_center(&w, &mut s, z); }
    }));
    win.on_fit_page(cb!(|w, s| {
        if let Some(z) = fit_zoom(&w, &s, true) { zoom_center(&w, &mut s, z); }
    }));

    win.on_zoom_wheel({
        let (weak, st, sch) = (win.as_weak(), st.clone(), schedule.clone());
        move |delta, sx, sy| {
            let Some(w) = weak.upgrade() else { return };
            {
                let Ok(mut s) = st.try_borrow_mut() else { return };
                let f = (delta * 0.002).exp().clamp(0.8, 1.25);
                let z = s.zoom * f;
                zoom_to(&w, &mut s, z, sx, sy);
            }
            sch();
        }
    });

    win.on_goto_page({
        let (weak, st) = (win.as_weak(), st.clone());
        move |i| {
            let Some(w) = weak.upgrade() else { return };
            let Ok(mut s) = st.try_borrow_mut() else { return };
            let Some(layout) = s.layout.clone() else { return };
            let (_, ph, gap) = geometry(&s);
            let content_h = layout.pages.len() as f32 * (ph + gap) + gap;
            let y = -(i.max(0) as f32 * (ph + gap));
            w.set_scroll_y(y.max((w.get_view_h() - content_h).min(0.0)));
            update_visible(&w, &mut s, false);
        }
    });

    win.on_viewport_changed({
        let (weak, st, sch) = (win.as_weak(), st.clone(), schedule.clone());
        move || {
            let Some(w) = weak.upgrade() else { return };
            {
                let Ok(mut s) = st.try_borrow_mut() else { return };
                if s.pending_fit && w.get_view_w() > 200.0 {
                    s.pending_fit = false;
                    if let Some(z) = fit_zoom(&w, &s, false) {
                        zoom_center(&w, &mut s, z.min(1.25));
                    }
                }
                update_visible(&w, &mut s, false);
            }
            sch();
        }
    });

    win.on_open_file({
        let (weak, st, sch) = (win.as_weak(), st.clone(), schedule.clone());
        move || {
            let Some(w) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("UYAP belgesi", &["udf"]).pick_file() else {
                return;
            };
            {
                let Ok(mut s) = st.try_borrow_mut() else { return };
                if let Err(e) = load(&w, &mut s, &path) {
                    w.set_status(format!("Hata: {e}").into());
                }
            }
            sch();
        }
    });

    if let Some(p) = std::env::args().nth(1) {
        let mut s = st.borrow_mut();
        if let Err(e) = load(&win, &mut s, Path::new(&p)) {
            win.set_status(format!("Hata: {e}").into());
        }
    }

    win.run()
}
