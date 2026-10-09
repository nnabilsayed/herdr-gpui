//! The taskbar overlay carrying the attention count: Windows' counterpart of
//! the Dock badge. A red disc with the count, or `9+` past nine, is drawn in
//! memory so no icon asset is needed, and an overlay icon is the supported way
//! to badge a taskbar button.

use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::{
            Shell::{ITaskbarList3, TaskbarList},
            WindowsAndMessaging::{CreateIcon, DestroyIcon, HICON},
        },
    },
    core::{PCWSTR, w},
};

const SIZE: usize = 32;
const DISC: [u8; 4] = [0x26, 0x26, 0xDC, 0xFF]; // BGRA: red
const INK: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// 3x5 glyphs for `0`-`9` then `+`, one row per entry, high bit on the left.
const GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b010, 0b111, 0b010, 0b000],
];

/// Sets, or with a zero count clears, the overlay on one window's taskbar
/// button. The badge is a courtesy, so a COM failure is dropped.
#[cfg_attr(test, allow(dead_code))]
pub(super) fn set(hwnd: isize, count: usize) {
    let _ = try_set(HWND(hwnd as *mut _), count);
}

#[cfg_attr(test, allow(dead_code))]
#[allow(unsafe_code)]
fn try_set(hwnd: HWND, count: usize) -> windows::core::Result<()> {
    // SAFETY: GPUI initializes COM on the UI thread, which is the only caller.
    // The icon outlives the call that copies it and is destroyed right after.
    unsafe {
        let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
        taskbar.HrInit()?;
        if count == 0 {
            return taskbar.SetOverlayIcon(hwnd, HICON::default(), PCWSTR::null());
        }
        let icon = icon(count)?;
        let description = if count == 1 {
            w!("1 agent needs you")
        } else {
            w!("Agents need you")
        };
        let result = taskbar.SetOverlayIcon(hwnd, icon, description);
        let _ = DestroyIcon(icon);
        result
    }
}

#[cfg_attr(test, allow(dead_code))]
#[allow(unsafe_code)]
fn icon(count: usize) -> windows::core::Result<HICON> {
    let xor = pixels(count);
    // A 1bpp AND mask of zeros: the 32bpp alpha channel decides coverage.
    let and = [0u8; SIZE * SIZE / 8];
    // SAFETY: both buffers are exactly the 32x32 sizes the call is told.
    unsafe {
        CreateIcon(
            None,
            SIZE as i32,
            SIZE as i32,
            1,
            32,
            and.as_ptr(),
            xor.as_ptr(),
        )
    }
}

/// BGRA pixels of the badge for `count`, top row first.
fn pixels(count: usize) -> Vec<u8> {
    let mut out = vec![0u8; SIZE * SIZE * 4];
    let radius = SIZE as f32 / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 + 0.5 - radius, y as f32 + 0.5 - radius);
            if dx * dx + dy * dy <= radius * radius {
                out[(y * SIZE + x) * 4..][..4].copy_from_slice(&DISC);
            }
        }
    }
    let glyphs: Vec<usize> = if count > 9 { vec![9, 10] } else { vec![count] };
    // One digit is drawn larger than the pair `9+`.
    let scale = if glyphs.len() == 1 { 4 } else { 3 };
    let width = (glyphs.len() * 4 - 1) * scale;
    let (left, top) = ((SIZE - width) / 2, (SIZE - 5 * scale) / 2);
    for (index, glyph) in glyphs.into_iter().enumerate() {
        for (row, bits) in GLYPHS[glyph].iter().enumerate() {
            for col in 0..3 {
                if bits >> (2 - col) & 1 == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let x = left + (index * 4 + col) * scale + sx;
                        let y = top + row * scale + sy;
                        out[(y * SIZE + x) * 4..][..4].copy_from_slice(&INK);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        let start = (y * SIZE + x) * 4;
        [
            pixels[start],
            pixels[start + 1],
            pixels[start + 2],
            pixels[start + 3],
        ]
    }

    #[test]
    fn badge_is_a_red_disc_with_transparent_corners() {
        let pixels = pixels(2);
        assert_eq!(at(&pixels, 0, 0)[3], 0);
        assert_eq!(at(&pixels, SIZE - 1, SIZE - 1)[3], 0);
        assert_eq!(at(&pixels, 4, SIZE / 2), DISC);
    }

    #[test]
    fn every_count_draws_ink_inside_the_disc() {
        for count in [1, 5, 9, 10, 99] {
            let pixels = pixels(count);
            assert!(pixels.chunks(4).any(|pixel| pixel == INK), "count {count}");
        }
    }

    #[test]
    fn digits_differ_and_ten_plus_shows_the_nine_plus_pair() {
        assert_ne!(pixels(1), pixels(2));
        assert_eq!(pixels(10), pixels(99));
        assert_ne!(pixels(10), pixels(9));
    }
}
