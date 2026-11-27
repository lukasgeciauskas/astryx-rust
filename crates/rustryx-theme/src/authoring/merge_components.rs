//! Component style maps and their merge: upstream `theme/mergeComponents.ts`.
//!
//! A theme's `components` map a component name to its style keys (`base`,
//! `variant:secondary`, `variant:destructive+size:sm`), and each style key to
//! CSS properties, pseudo-class blocks (`:hover`) and color-mode blocks
//! (`@light`, `@dark`). One merge rule serves `extends`, the type scale's
//! generated rules and the media surfaces: [`deep_merge_components`] merges per
//! style key and per property, a restated pseudo-class block replaces the
//! base's whole, and a color-mode block merges by property too.
//!
//! A key whose value is [`StyleValue::Unset`] (upstream's `undefined`) is not
//! set, at any depth: [`without_unset_components`] drops it, and with it a
//! style key or component left with nothing set.

use super::define::OrderedMap;

/// A value in a component style map (upstream's `StyleValue`): a CSS value, a
/// block of further keys, `null`, or `undefined`.
#[derive(Clone, Debug, PartialEq)]
pub enum StyleValue {
    /// A CSS value.
    Css(String),
    /// A block of keys: a component's style keys, a style key's properties, or
    /// a pseudo-class or color-mode block.
    Block(StyleMap),
    /// `null`: a value that is set, and replaces an inherited one.
    Null,
    /// `undefined`: the key sets nothing.
    Unset,
}

impl StyleValue {
    /// The block, or `None` for anything else.
    pub fn block(&self) -> Option<&StyleMap> {
        match self {
            StyleValue::Block(map) => Some(map),
            _ => None,
        }
    }

    /// The CSS value, or `None` for anything else.
    pub fn css(&self) -> Option<&str> {
        match self {
            StyleValue::Css(css) => Some(css),
            _ => None,
        }
    }
}

impl From<&str> for StyleValue {
    fn from(css: &str) -> Self {
        StyleValue::Css(css.to_string())
    }
}

impl From<String> for StyleValue {
    fn from(css: String) -> Self {
        StyleValue::Css(css)
    }
}

impl From<StyleMap> for StyleValue {
    fn from(map: StyleMap) -> Self {
        StyleValue::Block(map)
    }
}

/// Keys and their style values, in order.
pub type StyleMap = OrderedMap<StyleValue>;

/// A theme's component styles (upstream `ComponentStyleMap`): component name
/// to a [`StyleValue::Block`] of style keys, each a block of properties.
pub type ComponentStyleMap = StyleMap;

/// The style keys that hold a block for one color mode. Upstream
/// `COLOR_MODE_KEYS`.
pub const COLOR_MODE_KEYS: [&str; 2] = ["@light", "@dark"];

/// `map` with each entry read through `read`, dropping an entry that is
/// [`StyleValue::Unset`] or reads as `None`; `None` when a map with entries
/// loses them all. Upstream `readEntries` on an object.
fn read_map(
    map: &StyleMap,
    read: &dyn Fn(&StyleValue, &str) -> Option<StyleValue>,
) -> Option<StyleMap> {
    let mut kept = StyleMap::new();
    for (key, entry) in map.iter() {
        if matches!(entry, StyleValue::Unset) {
            continue;
        }
        if let Some(set) = read(entry, key) {
            kept.insert(key, set);
        }
    }
    if !map.is_empty() && kept.is_empty() {
        return None;
    }
    Some(kept)
}

/// `value` read through `read` when it is a block, and unchanged otherwise
/// (upstream `readEntries`: a null or a non-object returns as it is).
fn read_value(
    value: &StyleValue,
    read: &dyn Fn(&StyleValue, &str) -> Option<StyleValue>,
) -> Option<StyleValue> {
    match value {
        StyleValue::Block(map) => read_map(map, read).map(StyleValue::Block),
        StyleValue::Unset => None,
        other => Some(other.clone()),
    }
}

/// A style key's value without the properties it leaves unset, at every
/// depth. A pseudo-class block that empties stays, empty, since a restated
/// pseudo-class block still replaces the base's; a color-mode block that
/// empties is not set, as a property is not. Upstream `withoutUndefined`.
fn without_undefined(value: &StyleValue, key: &str) -> Option<StyleValue> {
    let set = read_value(value, &without_undefined);
    if set.is_none() && !COLOR_MODE_KEYS.contains(&key) {
        return Some(StyleValue::Block(StyleMap::new()));
    }
    set
}

/// A component style map without what it leaves unset (upstream
/// `withoutUnsetComponents`): a component, a style key or a property at any
/// depth that is [`StyleValue::Unset`] is not set, and neither is a component
/// or style key whose every value is. `None` for a map left with nothing.
pub fn without_unset_components(map: Option<&ComponentStyleMap>) -> Option<ComponentStyleMap> {
    let styles = |styles: &StyleValue, _: &str| read_value(styles, &without_undefined);
    let rules = |rules: &StyleValue, _: &str| read_value(rules, &styles);
    read_map(map?, &rules)
}

/// `{...value}` of a style value: a block's entries, nothing for anything
/// else.
fn entries_of(value: Option<&StyleValue>) -> StyleMap {
    value
        .and_then(StyleValue::block)
        .cloned()
        .unwrap_or_default()
}

/// Component style maps merged (upstream `deepMergeComponents`): every
/// component and style key of `inherited_map`, with `own_map`'s on top. Within
/// a style key the own properties win one by one, a pseudo-class block the own
/// map restates replaces the inherited one whole, and a color-mode block
/// (`@light`, `@dark`) merges by property. Both maps are read without what they
/// leave unset ([`without_unset_components`]); `None` when neither sets
/// anything.
pub fn deep_merge_components(
    inherited_map: Option<&ComponentStyleMap>,
    own_map: Option<&ComponentStyleMap>,
) -> Option<ComponentStyleMap> {
    let base = without_unset_components(inherited_map);
    let overrides = without_unset_components(own_map);
    let (base, overrides) = match (base, overrides) {
        (None, None) => return None,
        (None, overrides) => return overrides,
        (base, None) => return base,
        (Some(base), Some(overrides)) => (base, overrides),
    };

    // Start with all base entries.
    let mut result = ComponentStyleMap::new();
    for (component, rules) in base.iter() {
        result.insert(component, StyleValue::Block(entries_of(Some(rules))));
    }

    // Merge the overrides on top.
    for (component, rules) in overrides.iter() {
        let Some(StyleValue::Block(existing)) = result.get_mut(component) else {
            result.insert(component, StyleValue::Block(entries_of(Some(rules))));
            continue;
        };
        for (key, styles) in entries_of(Some(rules)).iter() {
            let inherited_styles = existing.get(key);
            let mut merged = entries_of(inherited_styles);
            merged.assign(&entries_of(Some(styles)));
            // A color-mode block merges by property too, as the style key does.
            for mode in COLOR_MODE_KEYS {
                let inherited = inherited_styles
                    .and_then(StyleValue::block)
                    .and_then(|styles| styles.get(mode));
                let own = styles.block().and_then(|styles| styles.get(mode));
                // `typeof null === 'object'`: a null block spreads nothing.
                let is_object = |value: Option<&StyleValue>| {
                    matches!(value, Some(StyleValue::Block(_) | StyleValue::Null))
                };
                if is_object(inherited) && is_object(own) {
                    let mut block = entries_of(inherited);
                    block.assign(&entries_of(own));
                    merged.insert(mode, StyleValue::Block(block));
                }
            }
            existing.insert(key, StyleValue::Block(merged));
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(value: &str) -> StyleValue {
        StyleValue::Css(value.to_string())
    }

    fn block<const N: usize>(entries: [(&str, StyleValue); N]) -> StyleValue {
        StyleValue::Block(entries.into())
    }

    fn map<const N: usize>(entries: [(&str, StyleValue); N]) -> ComponentStyleMap {
        entries.into()
    }

    /// Upstream's `base` map in mergeComponents.test.ts.
    fn base() -> ComponentStyleMap {
        map([
            (
                "card",
                block([(
                    "base",
                    block([
                        ("padding", css("8px")),
                        ("@dark", block([("color", css("white"))])),
                    ]),
                )]),
            ),
            ("button", block([("base", block([("margin", css("2px"))]))])),
        ])
    }

    #[test]
    fn keeps_the_inherited_component_style_key_and_value_an_override_leaves_unset() {
        let merged = deep_merge_components(
            Some(&base()),
            Some(&map([
                (
                    "card",
                    block([(
                        "base",
                        block([
                            ("padding", StyleValue::Unset),
                            ("@dark", block([("color", StyleValue::Unset)])),
                        ]),
                    )]),
                ),
                ("button", StyleValue::Unset),
                ("link", block([("base", StyleValue::Unset)])),
            ])),
        );
        // A component whose every style key is left unset is not set.
        assert_eq!(merged, Some(base()));
        assert!(!merged.unwrap().contains_key("link"));
    }

    #[test]
    fn drops_what_a_single_map_leaves_unset() {
        let merged = deep_merge_components(
            None,
            Some(&map([
                (
                    "card",
                    block([(
                        "base",
                        block([("padding", StyleValue::Unset), ("margin", css("4px"))]),
                    )]),
                ),
                ("button", StyleValue::Unset),
            ])),
        );
        assert_eq!(
            merged,
            Some(map([(
                "card",
                block([("base", block([("margin", css("4px"))]))])
            )]))
        );
    }

    #[test]
    fn keeps_null_as_a_value_that_replaces_the_inherited_one() {
        let merged = deep_merge_components(
            Some(&base()),
            Some(&map([(
                "card",
                block([("base", block([("padding", StyleValue::Null)]))]),
            )])),
        )
        .unwrap();
        let card_base = merged
            .get("card")
            .and_then(StyleValue::block)
            .and_then(|c| c.get("base"));
        assert_eq!(
            card_base,
            Some(&block([
                ("padding", StyleValue::Null),
                ("@dark", block([("color", css("white"))])),
            ]))
        );
    }

    #[test]
    fn own_properties_win_and_inherited_ones_stay() {
        // Upstream defineTheme.test.ts 'merges component overrides — child wins'.
        let merged = deep_merge_components(
            Some(&map([(
                "button",
                block([(
                    "base",
                    block([("fontWeight", css("600")), ("borderRadius", css("4px"))]),
                )]),
            )])),
            Some(&map([(
                "button",
                block([("base", block([("fontWeight", css("700"))]))]),
            )])),
        )
        .unwrap();
        assert_eq!(
            merged,
            map([(
                "button",
                block([(
                    "base",
                    block([("fontWeight", css("700")), ("borderRadius", css("4px"))]),
                )]),
            )])
        );
        // The inherited property keeps its place; the merged key lists first.
        let base = merged
            .get("button")
            .and_then(StyleValue::block)
            .and_then(|b| b.get("base"));
        let keys: Vec<&str> = base.and_then(StyleValue::block).unwrap().keys().collect();
        assert_eq!(keys, ["fontWeight", "borderRadius"]);
    }

    #[test]
    fn a_restated_pseudo_class_block_replaces_the_inherited_one_and_a_color_mode_block_merges() {
        let merged = deep_merge_components(
            Some(&map([(
                "card",
                block([(
                    "base",
                    block([
                        (
                            ":hover",
                            block([("color", css("red")), ("borderColor", css("blue"))]),
                        ),
                        (
                            "@dark",
                            block([("color", css("white")), ("borderColor", css("gray"))]),
                        ),
                    ]),
                )]),
            )])),
            Some(&map([(
                "card",
                block([(
                    "base",
                    block([
                        (":hover", block([("color", css("green"))])),
                        ("@dark", block([("color", css("black"))])),
                    ]),
                )]),
            )])),
        )
        .unwrap();
        assert_eq!(
            merged,
            map([(
                "card",
                block([(
                    "base",
                    block([
                        (":hover", block([("color", css("green"))])),
                        (
                            "@dark",
                            block([("color", css("black")), ("borderColor", css("gray"))])
                        ),
                    ]),
                )]),
            )])
        );
    }

    #[test]
    fn an_emptied_pseudo_class_block_stays_and_an_emptied_color_mode_block_goes() {
        let cleaned = without_unset_components(Some(&map([(
            "card",
            block([(
                "base",
                block([
                    ("padding", css("4px")),
                    (":hover", block([("color", StyleValue::Unset)])),
                    ("@dark", block([("color", StyleValue::Unset)])),
                ]),
            )]),
        )])));
        assert_eq!(
            cleaned,
            Some(map([(
                "card",
                block([(
                    "base",
                    block([("padding", css("4px")), (":hover", block([]))]),
                )]),
            )]))
        );
    }

    #[test]
    fn neither_map_setting_anything_merges_to_none() {
        assert_eq!(deep_merge_components(None, None), None);
        assert_eq!(
            deep_merge_components(None, Some(&map([("card", StyleValue::Unset)]))),
            None
        );
        // An empty map has nothing to lose, so it stays.
        assert_eq!(deep_merge_components(None, Some(&map([]))), Some(map([])));
    }
}
