//! Color math that Astryx does in CSS at draw time.
//!
//! - Hover and press paint a translucent overlay token over a fill
//!   (`utils/interactionOverlay.stylex.ts`): [`composite`].
//! - Indicators, Link, Switch and Slider tint with
//!   `color-mix(in srgb, X, var(--color-tint-hover) 15%)`: [`mix_srgb`].

use gpui::{Hsla, Rgba};

/// `top` painted over `base` (source-over alpha compositing).
pub fn composite(base: Hsla, top: Hsla) -> Hsla {
    let b = base.to_rgb();
    let t = top.to_rgb();
    let alpha = t.a + b.a * (1.0 - t.a);
    if alpha <= 0.0 {
        return gpui::transparent_black();
    }
    let channel = |tc: f32, bc: f32| (tc * t.a + bc * b.a * (1.0 - t.a)) / alpha;
    Rgba {
        r: channel(t.r, b.r),
        g: channel(t.g, b.g),
        b: channel(t.b, b.b),
        a: alpha,
    }
    .into()
}

/// `color-mix(in srgb, a, b <amount>)`: `amount` of `b` mixed into `a`,
/// interpolated with premultiplied alpha as CSS Color 5 specifies.
pub fn mix_srgb(a: Hsla, b: Hsla, amount: f32) -> Hsla {
    let amount = amount.clamp(0.0, 1.0);
    let a = a.to_rgb();
    let b = b.to_rgb();
    let alpha = a.a * (1.0 - amount) + b.a * amount;
    if alpha <= 0.0 {
        return gpui::transparent_black();
    }
    let channel = |ac: f32, bc: f32| (ac * a.a * (1.0 - amount) + bc * b.a * amount) / alpha;
    Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: alpha,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::rgba;

    fn hex(color: Hsla) -> String {
        let c = color.to_rgb();
        let byte = |v: f32| (v * 255.0).round() as u8;
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            byte(c.r),
            byte(c.g),
            byte(c.b),
            byte(c.a)
        )
    }

    #[test]
    fn mix_matches_css() {
        // CSS Color 5: color-mix(in srgb, red, blue) is #800080.
        let mixed = mix_srgb(rgba(0xff0000ff).into(), rgba(0x0000ffff).into(), 0.5);
        assert_eq!(hex(mixed), "#800080FF");
    }

    #[test]
    fn composite_paints_overlay() {
        // White under black at 0x1A (26/255) alpha: 255 * (1 - 26/255) = 229.
        let pressed = composite(rgba(0xffffffff).into(), rgba(0x0000001a).into());
        assert_eq!(hex(pressed), "#E5E5E5FF");
    }
}
