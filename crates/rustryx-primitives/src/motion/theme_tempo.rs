//! The tempo motion plays at, upstream `motion/themeTempo.ts`: a theme that
//! sets `tempo` scales every motion role by it (upstream writes it to the
//! private property `--_astryx-motion-tempo`; here the caller reads its
//! theme's tempo and hands it in).

/// The tempo motion plays at for a theme's tempo value: the value when it
/// is a positive finite number, else 1 (no theme, no tempo, or a value that
/// is not a positive number).
pub fn theme_tempo(value: Option<f64>) -> f64 {
    match value {
        Some(tempo) if tempo.is_finite() && tempo > 0. => tempo,
        _ => 1.,
    }
}

/// [`theme_tempo`] for the text a theme writes (upstream reads the custom
/// property's text): an empty or non-numeric text is 1.
pub fn theme_tempo_from_text(text: &str) -> f64 {
    theme_tempo(text.trim().parse::<f64>().ok())
}
