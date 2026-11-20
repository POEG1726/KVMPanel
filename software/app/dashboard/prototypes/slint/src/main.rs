//! Slint prototype of the PC-case dashboard main screen.
//!
//! Env:
//!   KVMPANEL_PANEL_ADDR   panel socket address (default 127.0.0.1:7701)
//!   PROTO_SCREENSHOT      if set, save a window snapshot PNG there after PROTO_SCREENSHOT_AFTER s (default 8) and quit
//!   SLINT_BACKEND=winit-software, SLINT_SCALE_FACTOR=0.5 to fit the 1080x1920 canvas on screen

mod blobs;
mod feed;

use slint::{Color, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::time::Duration;

slint::include_modules!();

thread_local! {
    static BLOBS: blobs::Profile = blobs::Profile::new();
    static INTENSITY: RefCell<f32> = const { RefCell::new(-1.0) };
}

/// Updates a model in place when the length is unchanged, so repeated items keep their
/// state and `animate` blocks run; replaces it otherwise.
fn sync<T: Clone + 'static>(current: ModelRc<T>, items: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    if current.row_count() == items.len() {
        for (i, item) in items.into_iter().enumerate() {
            current.set_row_data(i, item);
        }
    } else {
        set(ModelRc::new(VecModel::from(items)));
    }
}

fn rows(v: Vec<feed::RowData>) -> Vec<Row> {
    v.into_iter()
        .map(|r| Row {
            kind: r.kind,
            label: r.label.into(),
            value: r.value.into(),
            hot: r.hot,
            line: r.line.into(),
            area: r.area.into(),
            bar: r.bar,
        })
        .collect()
}

fn set_blobs(ui: &Dashboard, load: f32) {
    // mockup: --int = .3 + .5 * max(cpu, gpu load)
    let intensity = 0.3 + 0.5 * load.clamp(0.0, 1.0);
    let changed = INTENSITY.with(|i| {
        let mut i = i.borrow_mut();
        let changed = (*i - intensity).abs() > 0.004;
        if changed {
            *i = intensity;
        }
        changed
    });
    if !changed {
        return;
    }
    BLOBS.with(|p| {
        ui.set_blob0(p.image(blobs::COLORS[0], intensity));
        ui.set_blob1(p.image(blobs::COLORS[1], intensity));
        ui.set_blob2(p.image(blobs::COLORS[2], intensity));
    });
}

fn rgb(c: u32) -> Color {
    Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn apply(ui: &Dashboard, update: feed::Update) {
    match update {
        feed::Update::Telemetry(t) => {
            sync(ui.get_cpu_rows(), rows(t.cpu), |m| ui.set_cpu_rows(m));
            sync(ui.get_gpu_rows(), rows(t.gpu), |m| ui.set_gpu_rows(m));
            sync(ui.get_mem_rows(), rows(t.mem), |m| ui.set_mem_rows(m));
            sync(ui.get_sys_rows(), rows(t.sys), |m| ui.set_sys_rows(m));
            sync(ui.get_cores(), t.cores, |m| ui.set_cores(m));
            let dimms = t.dimms.into_iter().map(|d| Dimm { text: d.text.into(), present: d.present, hot: d.hot }).collect();
            sync(ui.get_dimms(), dimms, |m| ui.set_dimms(m));
            let series = t
                .series
                .into_iter()
                .map(|s| Series { name: s.name.into(), line: s.line.into(), area: s.area.into(), color: rgb(s.color) })
                .collect();
            sync(ui.get_dimm_series(), series, |m| ui.set_dimm_series(m));
            ui.set_dimm_range(SharedString::from(t.dimm_range));
            ui.set_clocks(SharedString::from(t.clocks));
            set_blobs(ui, t.load);
        }
        feed::Update::Status(chips) => {
            let chips = chips.into_iter().map(|(text, state)| Chip { text: text.into(), state }).collect();
            sync(ui.get_chips(), chips, |m| ui.set_chips(m));
        }
        feed::Update::Waiting(w) => ui.set_waiting(w),
    }
}

fn save_png(path: &str, buf: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, buf.width(), buf.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(buf.as_bytes()).map_err(std::io::Error::other)?;
    Ok(())
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = Dashboard::new()?;
    set_blobs(&ui, 0.0);

    let addr = std::env::var("KVMPANEL_PANEL_ADDR").unwrap_or_else(|_| "127.0.0.1:7701".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        feed::run(addr, move |update| {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    apply(&ui, update);
                }
            });
        })
    });

    let shot_timer = slint::Timer::default();
    if let Ok(path) = std::env::var("PROTO_SCREENSHOT") {
        let after: u64 = std::env::var("PROTO_SCREENSHOT_AFTER").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
        let weak = ui.as_weak();
        shot_timer.start(slint::TimerMode::SingleShot, Duration::from_secs(after), move || {
            let Some(ui) = weak.upgrade() else { return };
            match ui.window().take_snapshot() {
                Ok(buf) => {
                    eprintln!("snapshot {}x{} -> {path}", buf.width(), buf.height());
                    if let Err(e) = save_png(&path, &buf) {
                        eprintln!("saving {path}: {e}");
                    }
                }
                Err(e) => eprintln!("take_snapshot failed: {e}"),
            }
            let _ = slint::quit_event_loop();
        });
    }

    ui.run()
}
