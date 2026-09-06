//! Colors from DESIGN.md and their conversion for the GPU.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

/// Default canvas color, `canvas` in the light theme of DESIGN.md.
pub const CANVAS: u32 = 0xe3_d9_c3;

/// Builds an opaque GPU clear color from a `0xRRGGBB` sRGB value.
pub fn linear_color(rgb: u32) -> wgpu::Color {
    wgpu::Color {
        r: linear_from_srgb((rgb >> 16) as u8),
        g: linear_from_srgb((rgb >> 8) as u8),
        b: linear_from_srgb(rgb as u8),
        a: 1.0,
    }
}

/// Converts one 8-bit sRGB channel to linear light.
///
/// The surface format is sRGB, so clear colors must be given in linear light
/// for the displayed color to match the hex value in DESIGN.md.
pub fn linear_from_srgb(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    // Constants from the sRGB transfer function (IEC 61966-2-1).
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::{linear_color, linear_from_srgb};

    #[test]
    fn splits_hex_into_linear_channels() {
        let color = linear_color(0x00_80_ff);
        assert!(color.r.abs() < 1e-9);
        assert!((color.g - linear_from_srgb(0x80)).abs() < 1e-9);
        assert!((color.b - 1.0).abs() < 1e-9);
        assert!((color.a - 1.0).abs() < 1e-9);
    }

    #[test]
    fn maps_black_and_white_to_the_ends() {
        assert!(linear_from_srgb(0).abs() < 1e-9);
        assert!((linear_from_srgb(255) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn follows_the_srgb_curve() {
        // Reference values from the sRGB transfer function.
        assert!((linear_from_srgb(0xe3) - 0.768).abs() < 0.002);
        assert!((linear_from_srgb(0x80) - 0.216).abs() < 0.002);
        assert!((linear_from_srgb(0x0a) - 0.003).abs() < 0.001);
    }
}
