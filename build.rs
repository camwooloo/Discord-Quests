// Build script: generate the Aurora logo as an .ico and embed it as the exe's
// file icon so it shows in Explorer / the taskbar. Best-effort — if a Windows
// resource compiler isn't available, the build still succeeds (the runtime
// window icon in main.rs covers the taskbar while running).

fn main() {
    #[cfg(windows)]
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    let s = 256usize;
    let rgba = render_logo(s);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = std::path::Path::new(&out_dir).join("icon.ico");

    let image = ico::IconImage::from_rgba_data(s as u32, s as u32, rgba);
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    if let Ok(entry) = ico::IconDirEntry::encode(&image) {
        dir.add_entry(entry);
        if let Ok(file) = std::fs::File::create(&ico_path) {
            if dir.write(file).is_ok() {
                let mut res = winresource::WindowsResource::new();
                res.set_icon(ico_path.to_str().unwrap());
                let _ = res.compile(); // best-effort; ignore if rc tooling is absent
            }
        }
    }
}

/// Same aurora sparkle as the runtime window icon (kept in sync by hand).
#[cfg(windows)]
fn render_logo(s: usize) -> Vec<u8> {
    let sf = s as f32;
    let (cx, cy) = (sf / 2.0, sf / 2.0);
    let margin = sf * 0.06;
    let hw = sf / 2.0 - margin;
    let rad = sf * 0.23;
    let spark_r = hw * 0.64;
    let (pr, pg, pb) = (183.0f32, 148.0, 246.0);
    let (tr, tg, tb) = (52.0f32, 211.0, 153.0);
    let mut rgba = vec![0u8; s * s * 4];
    for y in 0..s {
        for x in 0..s {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (px, py) = ((fx - cx).abs(), (fy - cy).abs());
            let (qx, qy) = (px - (hw - rad), py - (hw - rad));
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad;
            let a = (0.5 - d).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let t = ((fx + fy) / (2.0 * sf)).clamp(0.0, 1.0);
            let mut r = pr + (tr - pr) * t;
            let mut g = pg + (tg - pg) * t;
            let mut b = pb + (tb - pb) * t;
            let nx = ((fx - cx) / spark_r).abs();
            let ny = ((fy - cy) / spark_r).abs();
            let star = nx.powf(0.42) + ny.powf(0.42);
            let sp = (1.0 - (star - 1.0) * 6.0).clamp(0.0, 1.0);
            r += (255.0 - r) * sp;
            g += (255.0 - g) * sp;
            b += (255.0 - b) * sp;
            let i = (y * s + x) * 4;
            rgba[i] = r as u8;
            rgba[i + 1] = g as u8;
            rgba[i + 2] = b as u8;
            rgba[i + 3] = (a * 255.0) as u8;
        }
    }
    rgba
}
