//! Colors from DESIGN.md and their conversion for the GPU.

// Rust guideline compliant 2026-02-21

/// Converts one 8-bit sRGB channel to linear light.
///
/// The surface format is sRGB, so clear colors must be given in linear light
/// for the displayed color to match the hex value in DESIGN.md.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used once the GPU clears the canvas")
)]
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
    use super::linear_from_srgb;

    #[test]
    fn maps_black_and_white_to_the_ends() {
        assert_eq!(linear_from_srgb(0), 0.0);
        assert_eq!(linear_from_srgb(255), 1.0);
    }

    #[test]
    fn follows_the_srgb_curve() {
        // Reference values from the sRGB transfer function.
        assert!((linear_from_srgb(0xe3) - 0.760).abs() < 0.002);
        assert!((linear_from_srgb(0x80) - 0.216).abs() < 0.002);
        assert!((linear_from_srgb(0x0a) - 0.003).abs() < 0.001);
    }
}
