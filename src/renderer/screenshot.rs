fn expand_tilde(path: &str) -> std::path::PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(rest),
        None => std::path::PathBuf::from(path),
    }
}

fn pixel_to_rgb(p: u32) -> [u8; 3] {
    [
        ((p >> 16) & 0xff) as u8,
        ((p >> 8) & 0xff) as u8,
        (p & 0xff) as u8,
    ]
}

pub(crate) fn sanitize_screenshot_name(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            ' ' => '-',
            c => c,
        })
        .collect()
}

pub(crate) fn save_screenshot(
    buf: &[u32],
    buf_width: u32,
    crop: [u32; 4],
    dir: &str,
    name: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let [x, y, w, h] = crop;
    let dir = expand_tilde(dir);
    use anyhow::Context as _;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("cannot create screenshot directory {}", dir.display()))?;

    let rgb: Vec<u8> = (y..y + h)
        .flat_map(|row| (x..x + w).map(move |col| (row * buf_width + col) as usize))
        .flat_map(|idx| pixel_to_rgb(buf.get(idx).copied().unwrap_or(0)))
        .collect();

    let filename = if name.trim().is_empty() {
        let timestamp = chrono::Local::now().format("%Y%m%dT%H%M%S");
        format!("mmterm-{timestamp}.png")
    } else {
        let sanitized = sanitize_screenshot_name(name.trim());
        format!("{sanitized}.png")
    };
    let path = dir.join(&filename);
    let img = image::RgbImage::from_raw(w, h, rgb)
        .ok_or_else(|| anyhow::anyhow!("invalid image dimensions {w}x{h}"))?;
    img.save(&path)
        .with_context(|| format!("cannot write PNG to {}", path.display()))?;
    log::info!("screenshot saved: {}", path.display());
    Ok(path)
}
