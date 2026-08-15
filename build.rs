// Build script: turn logo.png into an .ico and embed it as the exe's file icon
// so it shows in Explorer / the taskbar. Best-effort — if the PNG can't be read
// or a Windows resource compiler isn't available, the build still succeeds (the
// runtime window icon in main.rs covers the taskbar while running).

fn main() {
    println!("cargo:rerun-if-changed=src/logo.png");
    #[cfg(windows)]
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    let Some((rgba, w, h)) = load_logo() else { return };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = std::path::Path::new(&out_dir).join("icon.ico");

    let image = ico::IconImage::from_rgba_data(w, h, rgba);
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

/// Decode src/logo.png to RGBA (expanding RGB to RGBA if needed).
#[cfg(windows)]
fn load_logo() -> Option<(Vec<u8>, u32, u32)> {
    let bytes = std::fs::read("src/logo.png").ok()?;
    let mut reader = png::Decoder::new(&bytes[..]).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    match info.color_type {
        png::ColorType::Rgba => {
            buf.truncate((w * h * 4) as usize);
            Some((buf, w, h))
        }
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(px);
                out.push(255);
            }
            Some((out, w, h))
        }
        _ => None,
    }
}
