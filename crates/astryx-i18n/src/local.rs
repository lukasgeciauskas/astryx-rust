//! The port's own strings: text for what upstream leaves to the browser,
//! such as a text field's context menu (its spelling suggestions and
//! clipboard commands), and for spec items upstream has not built yet (a
//! PowerSearch value-only row). English only; an app translates them like
//! any key, with its catalogs or overrides.

const ENGLISH: &[(&str, &str)] = &[
    ("@astryx.list.reorderMoveLeft", "Move left"),
    ("@astryx.list.reorderMoveRight", "Move right"),
    ("@astryx.list.reorderNotMoved", "Can't move {label}"),
    (
        "@astryx.list.reorderPlaceInGrid",
        "{label}, row {row, number}, column {column, number}, position {position, number} of {total, number}",
    ),
    ("@astryx.listItem.moreActions", "More actions for {label}"),
    (
        "@astryx.powersearch.moreMatches",
        "More {field} matching ''{query}''",
    ),
    (
        "@astryx.powersearch.valueMatch",
        "{field} {operator} {value}",
    ),
    (
        "@astryx.powersearch.valueMatchText",
        "{field} {operator} \"{value}\"",
    ),
    ("@astryx.textField.addToDictionary", "Add to dictionary"),
    ("@astryx.textField.copy", "Copy"),
    ("@astryx.textField.cut", "Cut"),
    ("@astryx.textField.noSuggestions", "No spelling suggestions"),
    ("@astryx.textField.paste", "Paste"),
    ("@astryx.textField.selectAll", "Select all"),
];

/// English for one of the port's own keys.
pub(crate) fn english(key: &str) -> Option<&'static str> {
    ENGLISH
        .binary_search_by_key(&key, |(k, _)| k)
        .ok()
        .map(|i| ENGLISH[i].1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys_are_sorted_and_new() {
        assert!(super::ENGLISH.windows(2).all(|w| w[0].0 < w[1].0));
        for (key, _) in super::ENGLISH {
            assert!(
                crate::catalog::english(key).is_none(),
                "{key} is upstream's now: drop it here"
            );
        }
        assert_eq!(super::english("@astryx.textField.cut"), Some("Cut"));
    }
}
