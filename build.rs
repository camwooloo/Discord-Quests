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
                // winresource can't always locate the SDK's rc.exe on its own — point
                // it at the newest one we can find so the .exe file icon gets embedded.
                if let Some(rc_dir) = find_rc_dir() {
                    res.set_toolkit_path(&rc_dir);
                }
                match res.compile() {
                    Ok(_) => println!("cargo:warning=Aurora: embedded exe icon from logo.png"),
                    Err(e) => println!("cargo:warning=Aurora: exe icon embed failed: {e}"),
                }
            }
        }
    }
}

/// Newest `…\Windows Kits\10\bin\<ver>\x64` directory that contains rc.exe.
#[cfg(windows)]
fn find_rc_dir() -> Option<String> {
    for root in [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ] {
        let mut versions: Vec<_> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        versions.sort();
        for v in versions.into_iter().rev() {
            let dir = v.join("x64");
            if dir.join("rc.exe").exists() {
                return dir.to_str().map(str::to_string);
            }
        }
    }
    None
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
