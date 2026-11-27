//! Astryx's icons as embedded SVG assets.
//!
//! - `icons/core/<name>.svg`: core's fallback icons (24×24, 1.5px stroke),
//!   drawn when the theme maps no icon to a name.
//! - `icons/lucide/<name>.svg`: the lucide icons (ISC license) the published
//!   themes map semantic names to.
//!
//! GPUI loads SVGs through the app's [`AssetSource`], so an app serves these
//! files through [`IconAssets`], alone or in front of its own source.

use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

#[rustfmt::skip]
mod generated;

pub use generated::{CORE_ICONS, FILES};

/// The asset path of core's fallback icon for `name`, if core has one.
pub fn core_icon_path(name: &str) -> Option<String> {
    CORE_ICONS
        .contains(&name)
        .then(|| format!("icons/core/{}.svg", name.replace(':', "__")))
}

/// Serves the embedded icons, then defers to `fallback` for any other path.
pub struct IconAssets<A = ()> {
    fallback: A,
}

impl IconAssets<()> {
    pub fn new() -> Self {
        IconAssets { fallback: () }
    }
}

impl Default for IconAssets<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: AssetSource> IconAssets<A> {
    /// Serves the icons in front of the app's own asset source.
    pub fn with_fallback(fallback: A) -> Self {
        IconAssets { fallback }
    }
}

fn embedded(path: &str) -> Option<&'static [u8]> {
    FILES
        .binary_search_by_key(&path, |(p, _)| p)
        .ok()
        .map(|i| FILES[i].1)
}

fn list_embedded(prefix: &str) -> Vec<SharedString> {
    FILES
        .iter()
        .filter(|(p, _)| p.starts_with(prefix))
        .map(|(p, _)| SharedString::new_static(p))
        .collect()
}

impl<A: AssetSource> AssetSource for IconAssets<A> {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match embedded(path) {
            Some(bytes) => Ok(Some(Cow::Borrowed(bytes))),
            None => self.fallback.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = list_embedded(path);
        paths.extend(self.fallback.list(path)?);
        Ok(paths)
    }
}
