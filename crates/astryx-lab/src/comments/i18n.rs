//! Localized names for the Comments family's default reactions (upstream lab
//! `Comments/i18n.ts`).

use astryx_i18n::I18n;
use gpui::SharedString;

use super::types::CommentReactionOption;

/// Catalog keys for the names of the default reactions, by reaction key.
fn default_reaction_name_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "like" => "@astryx.comments.reactionLike",
        "love" => "@astryx.comments.reactionLove",
        "laugh" => "@astryx.comments.reactionLaugh",
        "wow" => "@astryx.comments.reactionWow",
        "sad" => "@astryx.comments.reactionSad",
        "celebrate" => "@astryx.comments.reactionCelebrate",
        _ => return None,
    })
}

/// Upstream `reactionDescription`: a reaction's human-readable name: the host's
/// `description`, else the localized name of a default reaction, else its glyph.
pub fn reaction_description(i18n: &I18n, reaction: &CommentReactionOption) -> SharedString {
    if let Some(description) = reaction.description.as_ref().filter(|d| !d.is_empty()) {
        return description.clone();
    }
    match default_reaction_name_key(&reaction.key) {
        Some(key) => i18n.t(key),
        None => reaction.label.clone(),
    }
}
