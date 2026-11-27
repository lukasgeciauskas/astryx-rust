//! UIKit's Dynamic Type table: upstream `theme/uikitTypography.ts`.
//!
//! The point size of each UIKit text style at each content size category, the
//! twelve columns in order: xSmall, Small, Medium, Large (the default), xLarge,
//! xxLarge, xxxLarge, then the five accessibility sizes AX1 to AX5 that Larger
//! Accessibility Sizes unlocks. A row is what
//! `UIFont.preferredFont(forTextStyle:compatibleWith:)` returns for that style,
//! and what Mobile Safari computes for the matching `font: -apple-system-*`
//! keyword (`title0` is Large Title, `title1` to `title3` are Title 1 to 3).
//!
//! Only the styles a theme role scales on are listed; upstream's
//! `theme/dynamicType.ts` maps each raw font-size step to one of them and
//! scales a theme's own type scale on these curves when the reader changes
//! their text size. Every row names its sources, as upstream's do.

/// A text style's point size at each content size category, xSmall to AX5.
pub type PointSizes = [f64; 12];

/// The point sizes of each UIKit text style a theme role scales on (upstream
/// `typography`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UikitTypography {
    /// Large Title, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-title0` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub large_title: PointSizes,
    /// Title 1, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-title1` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub title1: PointSizes,
    /// Title 2, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-title2` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub title2: PointSizes,
    /// Title 3, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-title3` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub title3: PointSizes,
    /// Body, in points per content size category (xSmall … AX5). Every
    /// category has its own Body size, so Body alone identifies the category.
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-body` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub body: PointSizes,
    /// Footnote, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-footnote` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub footnote: PointSizes,
    /// Caption 1, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-caption1` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub caption1: PointSizes,
    /// Caption 2, in points per content size category (xSmall … AX5).
    ///
    /// @source measured: UIKit lab (injected host app), iOS 26.2 simulator,
    ///   iPhone 17 Pro, UIFont.preferredFont(forTextStyle:compatibleWith:) at
    ///   each content size category, Oct 23 2026
    /// @source measured: Mobile Safari, iOS 26.2 simulator, iPhone 17 Pro,
    ///   computed font-size of `font: -apple-system-caption2` at each category,
    ///   Oct 24 2026
    /// @source apple-docs https://developer.apple.com/design/human-interface-guidelines/typography
    pub caption2: PointSizes,
}

impl UikitTypography {
    /// Every row with upstream's key (`largeTitle`, `title1`, …), in upstream's
    /// order (`Object.keys(typography)`).
    pub const fn rows(&self) -> [(&'static str, &PointSizes); 8] {
        [
            ("largeTitle", &self.large_title),
            ("title1", &self.title1),
            ("title2", &self.title2),
            ("title3", &self.title3),
            ("body", &self.body),
            ("footnote", &self.footnote),
            ("caption1", &self.caption1),
            ("caption2", &self.caption2),
        ]
    }

    /// The row upstream keys `style` (`typography[style]`), or `None` for a
    /// style the table does not list.
    pub fn row(&self, style: &str) -> Option<&PointSizes> {
        self.rows()
            .into_iter()
            .find(|(key, _)| *key == style)
            .map(|(_, row)| row)
    }
}

/// UIKit's point sizes per text style and content size category (upstream
/// `typography`). See [`UikitTypography`] for each row's sources.
pub const TYPOGRAPHY: UikitTypography = UikitTypography {
    large_title: [31., 32., 33., 34., 36., 38., 40., 44., 48., 52., 56., 60.],
    title1: [25., 26., 27., 28., 30., 32., 34., 38., 43., 48., 53., 58.],
    title2: [19., 20., 21., 22., 24., 26., 28., 34., 39., 44., 50., 56.],
    title3: [17., 18., 19., 20., 22., 24., 26., 31., 37., 43., 49., 55.],
    body: [14., 15., 16., 17., 19., 21., 23., 28., 33., 40., 47., 53.],
    footnote: [12., 12., 12., 13., 15., 17., 19., 23., 27., 33., 38., 44.],
    caption1: [11., 11., 11., 12., 14., 16., 18., 22., 26., 32., 37., 43.],
    caption2: [11., 11., 11., 11., 13., 15., 17., 20., 24., 29., 34., 40.],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The column of Large, the default category.
    const LARGE: usize = 3;

    // Upstream dynamicType.test.ts: "is named by the size of Body alone".
    #[test]
    fn body_alone_identifies_each_category() {
        let body = TYPOGRAPHY.body;
        for (index, size) in body.iter().enumerate() {
            let matches: Vec<usize> = body
                .iter()
                .enumerate()
                .filter(|(_, other)| (*other - size).abs() < 0.5)
                .map(|(column, _)| column)
                .collect();
            assert_eq!(matches, [index], "body {size}");
        }
        // Large is Body's 17pt, the UIKit default.
        assert_eq!(body[LARGE], 17.0);
    }

    // The table's keys, in upstream's order, and each row's lookup.
    #[test]
    fn rows_are_keyed_as_upstream() {
        let keys: Vec<&str> = TYPOGRAPHY.rows().iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                "largeTitle",
                "title1",
                "title2",
                "title3",
                "body",
                "footnote",
                "caption1",
                "caption2"
            ]
        );
        assert_eq!(TYPOGRAPHY.row("title3"), Some(&TYPOGRAPHY.title3));
        assert_eq!(TYPOGRAPHY.row("headline"), None);
    }

    // Read from the table: every style grows (or stays) from xSmall to AX5.
    #[test]
    fn every_row_grows_with_the_category() {
        for (style, row) in TYPOGRAPHY.rows() {
            assert!(row.windows(2).all(|pair| pair[0] <= pair[1]), "{style}");
        }
    }

    // Upstream dynamicType.test.ts: "names a source for every row, and the iOS
    // version and date of every measurement", over this file's doc comments.
    #[test]
    fn every_row_names_its_sources() {
        let source = include_str!("uikit_typography.rs");
        let body = &source[source.find("pub struct UikitTypography").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        let mut rows = 0;
        let mut doc: Vec<&str> = Vec::new();
        for line in body.lines().map(str::trim) {
            if let Some(text) = line.strip_prefix("///") {
                doc.push(text.trim());
                continue;
            }
            if let Some(field) = line.strip_prefix("pub ").and_then(|l| l.split_once(':')) {
                rows += 1;
                // A tag continues on the following lines until the next tag.
                let joined = doc.join(" ");
                let tags: Vec<&str> = joined.split("@source ").skip(1).map(str::trim).collect();
                assert!(!tags.is_empty(), "{}", field.0);
                for tag in tags {
                    let measured = tag.strip_prefix("measured: ");
                    assert!(
                        measured.is_some()
                            || tag.starts_with("apple-docs https://developer.apple.com/")
                            || tag.starts_with("wwdc ")
                            || tag.starts_with("webkit Source/"),
                        "{}: {tag}",
                        field.0
                    );
                    if let Some(measured) = measured {
                        let ios = measured
                            .find("iOS ")
                            .expect("a measurement names the iOS version");
                        assert!(measured[ios + 4..].starts_with(|c: char| c.is_ascii_digit()));
                        assert!(
                            measured.contains(" 20"),
                            "{}: a measurement names its date",
                            field.0
                        );
                    }
                }
            }
            doc.clear();
        }
        assert_eq!(rows, 8);
    }
}
