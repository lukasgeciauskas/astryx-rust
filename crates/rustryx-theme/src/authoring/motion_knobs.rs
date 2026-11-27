//! The motion knobs, tempo and character: upstream `theme/motionKnobs.ts`.
//!
//! A theme's `tempo` scales every motion duration and `character` picks how
//! lively its curves are (`spec:AST-108` FR1). [`resolve_knobs`] fills in the
//! defaults and clamps the tempo to [`TEMPO_RANGE`]. Every theme follows each
//! person's Reduce Motion setting: `reduced_motion` is not a public knob until
//! `Always` ships, so it always resolves to [`ThemeReducedMotion::System`].
//!
//! [`super::theme_motion`] builds the motion tokens on these; the play-time
//! engine in `rustryx_primitives::motion` takes the resolved tempo.

/// How lively a theme's motion feels (upstream `MotionCharacter`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MotionCharacter {
    /// Decelerates without overshoot.
    Crisp,
    /// Astryx's own curves.
    #[default]
    Standard,
    /// Overshoots slightly on entrances and moves. Exits and fades never
    /// overshoot.
    Expressive,
}

impl MotionCharacter {
    /// Every character, in upstream's order (upstream `CHARACTERS`).
    pub const ALL: [MotionCharacter; 3] = [Self::Crisp, Self::Standard, Self::Expressive];

    /// Upstream's name: `crisp`, `standard` or `expressive`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crisp => "crisp",
            Self::Standard => "standard",
            Self::Expressive => "expressive",
        }
    }

    /// The character upstream names `name`, or `None` for a name it does not
    /// list (upstream's `resolveKnobs` treats such a name as unset).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|character| character.as_str() == name)
    }
}

/// Whose Reduce Motion setting a theme follows (upstream `ThemeReducedMotion`):
/// each person's own, or reduced for everyone. Internal upstream: every theme
/// resolves to `System` until `Always` ships (`spec:AST-108` FR6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeReducedMotion {
    /// Each person's own setting.
    #[default]
    System,
    /// Reduced for everyone.
    Always,
}

impl ThemeReducedMotion {
    /// Upstream's name: `system` or `always`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Always => "always",
        }
    }
}

/// The motion knobs a theme sets (upstream `ThemeMotionKnobs`, `spec:AST-108`
/// FR1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThemeMotionKnobs {
    /// Scales every motion duration: 1 is Astryx's own, 0.75 the quickest and
    /// 1.5 the slowest (values outside are clamped; a value that is not finite
    /// counts as unset). Default 1.
    pub tempo: Option<f64>,
    /// How lively the curves are. Default [`MotionCharacter::Standard`].
    pub character: Option<MotionCharacter>,
}

/// The knobs with their defaults filled in and `tempo` clamped (upstream
/// `ResolvedMotionKnobs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedMotionKnobs {
    /// The tempo, within [`TEMPO_RANGE`].
    pub tempo: f64,
    pub character: MotionCharacter,
    pub reduced_motion: ThemeReducedMotion,
    /// Whether the theme sets `tempo` itself. A motion scale alone implies a
    /// tempo for its CSS durations only; the animator's springs stay at tempo 1
    /// unless this is true (FR5).
    pub tempo_is_set: bool,
}

/// The range `tempo` is clamped to (upstream `TEMPO_RANGE`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoRange {
    /// The quickest tempo.
    pub min: f64,
    /// The slowest tempo.
    pub max: f64,
}

/// The range `tempo` is clamped to: 0.75 to 1.5 (upstream `TEMPO_RANGE`).
pub const TEMPO_RANGE: TempoRange = TempoRange {
    min: 0.75,
    max: 1.5,
};

/// The knobs of `config`, defaults filled in and `tempo` clamped (upstream
/// `resolveKnobs`). A tempo counts as set when it is a finite number; a
/// `NaN` or infinite one resolves like no tempo, to 1.
pub fn resolve_knobs(config: Option<&ThemeMotionKnobs>) -> ResolvedMotionKnobs {
    let set_tempo = config
        .and_then(|config| config.tempo)
        .filter(|tempo| tempo.is_finite());
    let tempo_is_set = set_tempo.is_some();
    let tempo = set_tempo.map_or(1.0, |tempo| TEMPO_RANGE.max.min(TEMPO_RANGE.min.max(tempo)));
    let character = config
        .and_then(|config| config.character)
        .unwrap_or(MotionCharacter::Standard);
    ResolvedMotionKnobs {
        tempo,
        character,
        reduced_motion: ThemeReducedMotion::System,
        tempo_is_set,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempo(tempo: f64) -> ThemeMotionKnobs {
        ThemeMotionKnobs {
            tempo: Some(tempo),
            character: None,
        }
    }

    // Upstream themeMotion.test.ts: "default to tempo 1, standard and system".
    #[test]
    fn knobs_default_to_tempo_1_standard_and_system() {
        let expected = ResolvedMotionKnobs {
            tempo: 1.0,
            character: MotionCharacter::Standard,
            reduced_motion: ThemeReducedMotion::System,
            tempo_is_set: false,
        };
        assert_eq!(resolve_knobs(None), expected);
        assert_eq!(resolve_knobs(Some(&ThemeMotionKnobs::default())), expected);
    }

    // Upstream: "clamp tempo to 0.75–1.5 and ignore a value that is no number".
    #[test]
    fn tempo_clamps_and_ignores_a_non_finite_value() {
        assert_eq!(resolve_knobs(Some(&tempo(0.5))).tempo, 0.75);
        assert_eq!(resolve_knobs(Some(&tempo(2.0))).tempo, 1.5);
        assert_eq!(resolve_knobs(Some(&tempo(1.2))).tempo, 1.2);
        let nan = resolve_knobs(Some(&tempo(f64::NAN)));
        assert_eq!((nan.tempo, nan.tempo_is_set), (1.0, false));
        // `Number.isFinite` rejects an infinite tempo too.
        let infinite = resolve_knobs(Some(&tempo(f64::INFINITY)));
        assert_eq!((infinite.tempo, infinite.tempo_is_set), (1.0, false));
        // A clamped tempo still counts as set.
        assert!(resolve_knobs(Some(&tempo(3.0))).tempo_is_set);
    }

    // Upstream: "resolve reducedMotion to system, which no theme sets until
    // always ships".
    #[test]
    fn reduced_motion_resolves_to_system() {
        assert_eq!(
            resolve_knobs(Some(&ThemeMotionKnobs::default())).reduced_motion,
            ThemeReducedMotion::System
        );
    }

    #[test]
    fn character_names_round_trip() {
        for character in MotionCharacter::ALL {
            assert_eq!(
                MotionCharacter::from_name(character.as_str()),
                Some(character)
            );
        }
        assert_eq!(MotionCharacter::from_name("bouncy"), None);
        let expressive = ThemeMotionKnobs {
            tempo: None,
            character: Some(MotionCharacter::Expressive),
        };
        assert_eq!(
            resolve_knobs(Some(&expressive)).character,
            MotionCharacter::Expressive
        );
    }
}
