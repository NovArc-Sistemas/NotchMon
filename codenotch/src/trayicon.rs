//! Draws the usage readings into the tray icon itself, so the numbers are visible next to the
//! clock without opening the notch — and so the notch can be hidden entirely if the user prefers.
//!
//! No font dependency: a tray icon is 16x16 at 100 % scaling, and a hand-written 5x7 bitmap of the
//! ten digits is sharper at that size than any text renderer. Everything is drawn at 32x32 and
//! Windows halves it, an exact 2:1 reduction.
//!
//! EVERY offset below must be EVEN, and that is the whole trick. Each font pixel is drawn as a
//! 2x2 block; a block starting on an odd row or column straddles the 2x2 grid Windows averages
//! over when it halves the image, and the digit turns to grey mush. Aligned to even coordinates
//! each block maps to exactly one output pixel and the result is pixel-perfect. Anything that
//! changes the layout below has to preserve that.
//!
//! Why the digits are white rather than the band colour:
//!   Windows renders this icon at 16, 20 or 24 pixels depending on the display scaling, so the
//!   32x32 source is nearly always downscaled by a fraction rather than by half. Mid-tone coloured
//!   strokes turn to mush under that; white keeps its contrast against any taskbar. The band colour
//!   moves to a solid rule under each number, which is a shape thick enough to survive the resize.
//!
//! Why there are two layouts rather than one:
//!   A digit needs 7 of the 16 available pixels of height. Two rows fit (7 + 1 + 7 = 15). Three do
//!   not — each row would get 5 pixels and the digits would be unreadable. So `numbers` covers one
//!   or two providers, and `bars` covers more by dropping the digits for a column per provider,
//!   filled to its usage. That is a real constraint of the icon size, not a shortcut.

/// One row per scanline, the low 5 bits of each byte being the pixels, left-most bit first.
const FONT: [[u8; 7]; 10] = [
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110], // 0
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110], // 1
    [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111], // 2
    [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110], // 3
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010], // 4
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110], // 5
    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110], // 6
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000], // 7
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110], // 8
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100], // 9
];

const SIZE: usize = 32;
const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// How many readings the digit layout can show. See the module note: a third row would leave five
/// pixels per digit and nothing legible.
pub const MAX_NUMBER_ROWS: usize = 2;

/// The same three bands the rings use: comfortable, getting close, nearly spent.
fn band_color(pct: u32) -> [u8; 3] {
    if pct < 50 {
        [0x4a, 0xde, 0x80] // green
    } else if pct < 80 {
        [0xfa, 0xcc, 0x15] // amber
    } else {
        [0xf8, 0x71, 0x71] // red
    }
}

fn put(buf: &mut [u8], x: usize, y: usize, rgb: [u8; 3], a: u8) {
    if x >= SIZE || y >= SIZE {
        return;
    }
    let i = (y * SIZE + x) * 4;
    buf[i] = rgb[0];
    buf[i + 1] = rgb[1];
    buf[i + 2] = rgb[2];
    buf[i + 3] = a;
}

fn draw_digit(buf: &mut [u8], d: usize, ox: usize, oy: usize, s: usize, rgb: [u8; 3]) {
    let rows = &FONT[d.min(9)];
    for (ry, row) in rows.iter().enumerate() {
        for cx in 0..GLYPH_W {
            if row & (1 << (GLYPH_W - 1 - cx)) != 0 {
                for dy in 0..s {
                    for dx in 0..s {
                        put(buf, ox + cx * s + dx, oy + ry * s + dy, rgb, 255);
                    }
                }
            }
        }
    }
}

fn digits_of(pct: u32) -> Vec<usize> {
    if pct >= 100 {
        vec![1, 0, 0]
    } else if pct >= 10 {
        vec![(pct / 10) as usize, (pct % 10) as usize]
    } else {
        vec![pct as usize]
    }
}

const WHITE: [u8; 3] = [0xff, 0xff, 0xff];

/// The digits of `pct` in white, centred horizontally, top edge at `oy`.
fn draw_digits(buf: &mut [u8], pct: u32, oy: usize, s: usize) {
    let digits = digits_of(pct);
    // Three digits are only reached at 100 %, and only fit with the gap closed up
    let gap = if digits.len() >= 3 { 0 } else { 2 };
    let total_w = digits.len() * GLYPH_W * s + (digits.len() - 1) * gap;
    // Rounded DOWN to an even column: see the module note on why alignment decides sharpness
    let ox = (SIZE.saturating_sub(total_w) / 2) & !1;
    let mut x = ox;
    for d in &digits {
        draw_digit(buf, *d, x, oy, s, WHITE);
        x += GLYPH_W * s + gap;
    }
}

/// The band colour as a solid rule. Thick, flat shapes survive Windows' fractional downscaling of
/// this icon far better than coloured strokes do, which is why the colour lives here and not in
/// the digits.
fn draw_rule(buf: &mut [u8], pct: u32, y: usize) {
    let rgb = band_color(pct);
    // Two rows tall from an even row, so it halves into one solid output pixel rather than a
    // half-transparent one.
    for dy in 0..2 {
        for bx in 2..SIZE - 2 {
            put(buf, bx, y + dy, rgb, 255);
        }
    }
}

/// A dash, for a reading that is not available (provider absent, not signed in, not fetched yet).
fn draw_dash(buf: &mut [u8], oy: usize, s: usize) {
    let w = GLYPH_W * s;
    let ox = (SIZE.saturating_sub(w) / 2) & !1;
    let y = (oy + (GLYPH_H * s) / 2) & !1;
    for dx in 0..w {
        for dy in 0..s {
            put(buf, ox + dx, y + dy, [0x80, 0x80, 0x80], 200);
        }
    }
}

/// Digit layout: one reading filling the icon, or two stacked, each with its band colour as a rule
/// underneath. Anything past the first `MAX_NUMBER_ROWS` is ignored here — the caller decides
/// whether `bars` fits better, and the tooltip still lists every selected provider.
pub fn numbers(values: &[Option<u32>]) -> tauri::image::Image<'static> {
    let buf = numbers_rgba(values);
    tauri::image::Image::new_owned(buf, SIZE as u32, SIZE as u32)
}

/// The same pixels as `numbers`, as a raw RGBA buffer, so the settings preview can show exactly
/// what the taskbar will show rather than a second drawing that could drift out of step.
pub fn numbers_rgba(values: &[Option<u32>]) -> Vec<u8> {
    let mut buf = vec![0u8; SIZE * SIZE * 4]; // transparent

    if values.len() <= 1 {
        let v = values.first().copied().flatten();
        match v {
            Some(p) => {
                let p = p.min(100);
                // 100 needs three digits, which only fit one size down
                // Scale 2, not 3: an odd scale cannot line up with the 2x2 grid Windows averages
                // over, so a bigger-but-blurred digit reads worse than a smaller crisp one.
                const S: usize = 2;
                let oy = ((SIZE.saturating_sub(GLYPH_H * S)) / 2 - 2) & !1;
                draw_digits(&mut buf, p, oy, S);
                // A single number has room for a proper usage bar rather than a plain rule
                let rgb = band_color(p);
                let bar_y = SIZE - 6; // even
                let filled = ((SIZE as u32 * p / 100) as usize) & !1; // even, so the bar end is crisp
                for bx in 0..SIZE {
                    for by in bar_y..bar_y + 4 {
                        put(&mut buf, bx, by, rgb, if bx < filled { 255 } else { 60 });
                    }
                }
            }
            None => draw_dash(&mut buf, ((SIZE.saturating_sub(GLYPH_H * 2)) / 2 - 2) & !1, 2),
        }
        return buf;
    }

    // Two rows of 16: 14 pixels of digits at scale 2, then the colour rule on the 15th.
    const BAND_H: usize = GLYPH_H * 2; // 14
    for (row, v) in values.iter().take(MAX_NUMBER_ROWS).enumerate() {
        let top = row * 16; // 0 and 16, both even
        match v {
            Some(p) => {
                let p = (*p).min(100);
                // 100 is three digits. At scale 2 that is 32 pixels wide — the full icon, with no
                // margin, and Windows' downscaling then smears the digits into each other. Dropping
                // that row to scale 1 keeps it readable; it is centred in the same band so the two
                // rows stay aligned and the rules stay where they are.
                draw_digits(&mut buf, p, top, 2);
                draw_rule(&mut buf, p, top + BAND_H);
            }
            None => draw_dash(&mut buf, top, 2),
        }
    }
    buf
}

/// Column layout: one column per provider, filled from the bottom to its usage and coloured by
/// band. No digits, so it scales to more providers than the digit layout can hold — the exact
/// numbers stay available in the tooltip and in the notch itself.
pub fn bars(values: &[Option<u32>]) -> tauri::image::Image<'static> {
    let buf = bars_rgba(values);
    tauri::image::Image::new_owned(buf, SIZE as u32, SIZE as u32)
}

/// Raw RGBA for the column layout — see `numbers_rgba` for why this exists.
pub fn bars_rgba(values: &[Option<u32>]) -> Vec<u8> {
    let mut buf = vec![0u8; SIZE * SIZE * 4];
    let n = values.len().max(1);
    // One pixel of gap between columns; whatever is left over is shared out by integer division
    let gap = if n > 1 { 1 } else { 0 };
    let col_w = (SIZE - gap * (n - 1)) / n;
    if col_w == 0 {
        return buf;
    }
    let used_w = col_w * n + gap * (n - 1);
    let ox = (SIZE - used_w) / 2;

    for (i, v) in values.iter().enumerate() {
        let x0 = ox + i * (col_w + gap);
        match v {
            Some(p) => {
                let p = (*p).min(100);
                let rgb = band_color(p);
                let filled = (SIZE as u32 * p / 100) as usize;
                for y in 0..SIZE {
                    // y counts down from the top, so the fill grows upward from the bottom
                    let lit = SIZE - y <= filled;
                    for dx in 0..col_w {
                        put(&mut buf, x0 + dx, y, rgb, if lit { 255 } else { 45 });
                    }
                }
            }
            None => {
                // Not available: a dim track only, so an absent provider never looks like zero usage
                for y in 0..SIZE {
                    for dx in 0..col_w {
                        put(&mut buf, x0 + dx, y, [0x80, 0x80, 0x80], 35);
                    }
                }
            }
        }
    }
    buf
}

/// The icon as a `data:` URL, for the settings window's preview. The image is the real 32x32 one,
/// shown magnified with pixelated scaling, so what is being edited is literally what the taskbar
/// will draw.
pub fn to_data_url(rgba: &[u8]) -> Option<String> {
    let mut png: Vec<u8> = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, SIZE as u32, SIZE as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", b64_encode(&png)))
}

/// The application's own icon, at tray size.
///
/// NOT `icons/tray.png`: that one is a monochrome outline in pure black, which reads on a light
/// taskbar and is invisible on a dark one — the Windows 11 default. Recolouring it does not rescue
/// it either, because the shape is an outline with an empty middle: only 98 of its 1024 pixels are
/// fully opaque, so at 16 pixels it is a faint grey ring whichever colour it is drawn in.
/// `icons/tray-color.png` is the 32x32 frame lifted straight out of `icons/icon.ico`, the real
/// application icon, which carries its own colour and is legible on any taskbar.
pub fn app_mark() -> Option<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-color.png")).ok()
}

/// The same icon as a `data:` URL, so the settings window shows what the taskbar shows.
pub fn app_mark_data_url() -> Option<String> {
    png_data_url(include_bytes!("../icons/tray-color.png"))
}

/// Wraps PNG bytes that are already encoded (the bundled tray mark) as a `data:` URL.
pub fn png_data_url(png: &[u8]) -> Option<String> {
    if png.is_empty() {
        return None;
    }
    Some(format!("data:image/png;base64,{}", b64_encode(png)))
}

/// Base64 without pulling in a crate for it — the only encoder the app needs.
fn b64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}
