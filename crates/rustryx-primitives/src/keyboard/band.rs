//! Upstream `keyboard/revealBand.ts`: the scroll that brings a box inside a band, shared by
//! a field's reveal and the Tall sheet's reveal.

/// How far content must scroll so a box from `top` to `bottom` sits inside the band from
/// `safe_top` to `safe_bottom`, px. A positive distance scrolls it up and a negative one
/// down. It is 0 when the box fits or the band is empty. A box taller than the band keeps
/// its bottom edge in it.
pub fn distance_into_band(top: f64, bottom: f64, safe_top: f64, safe_bottom: f64) -> f64 {
    if safe_bottom <= safe_top {
        return 0.;
    }
    if bottom > safe_bottom {
        return bottom - safe_bottom;
    }
    if top < safe_top {
        return top - safe_top;
    }
    0.
}
