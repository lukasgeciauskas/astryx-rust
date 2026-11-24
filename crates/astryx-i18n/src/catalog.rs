//! Lookups in the generated catalogs (`generated/catalogs.rs`).

#[rustfmt::skip]
#[allow(clippy::all)]
mod generated {
    include!("generated/catalogs.rs");
}

#[cfg(test)]
pub use generated::KEYS;

fn index(key: &str) -> Option<usize> {
    generated::KEYS.binary_search(&key).ok()
}

/// English for a key.
pub fn english(key: &str) -> Option<&'static str> {
    index(key).map(|i| generated::ENGLISH[i])
}

/// A shipped translation of a key for an exact locale tag.
pub fn shipped(tag: &str, key: &str) -> Option<&'static str> {
    let (_, messages) = generated::SHIPPED
        .iter()
        .find(|(shipped, _)| shipped.eq_ignore_ascii_case(tag))?;
    let index = index(key)? as u16;
    messages
        .binary_search_by_key(&index, |(i, _)| *i)
        .ok()
        .map(|i| messages[i].1)
}

#[cfg(test)]
/// The tags of the shipped translations.
pub fn shipped_locales() -> impl Iterator<Item = &'static str> {
    generated::SHIPPED.iter().map(|(tag, _)| *tag)
}
