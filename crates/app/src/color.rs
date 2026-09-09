//! Colors from DESIGN.md and their conversion for the GPU.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;

/// Builds an opaque GPU clear color from a token of DESIGN.md 2.
pub fn linear_token(color: egui::Color32) -> wgpu::Color {
    wgpu::Color {
        r: linear_from_srgb(color.r()),
        g: linear_from_srgb(color.g()),
        b: linear_from_srgb(color.b()),
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
    use super::{linear_from_srgb, linear_token};

    #[test]
    fn splits_a_token_into_linear_channels() {
        let color = linear_token(egui::Color32::from_rgb(0x00, 0x80, 0xff));
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
