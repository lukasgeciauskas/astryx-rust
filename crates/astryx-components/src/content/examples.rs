//! Content examples: the gallery's Content page and the doc example of
//! Code, Kbd, Blockquote and Citation.

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::content::{Blockquote, Citation, CitationSource, CitationVariant, Code, Kbd};
use crate::layout::{SpacingStep, Stack, StackAlign};
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let row = || {
        Stack::horizontal()
            .gap(SpacingStep::S2)
            .v_align(StackAlign::Center)
    };
    let source = CitationSource {
        title: Some("Astryx design guidelines".into()),
        url: Some("https://example.com/guidelines".into()),
        src: None,
        icon: Some("externalLink".into()),
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Code"))
        .child(
            row()
                .child(Text::new("Run"))
                .child(Code::new("cargo xtask dev"))
                .child(Text::new("to start the edit loop.")),
        )
        .child(label("Kbd"))
        .child(
            row()
                .child(Kbd::new("mod+k").id("kbd-palette"))
                .child(Kbd::new("shift+?"))
                .child(Kbd::new("esc"))
                .child(Kbd::new("mod+shift+enter")),
        )
        .child(label("Blockquote"))
        .child(
            Blockquote::new()
                .child(Text::new(
                    "Design is not just what it looks like and feels like. Design is how it works.",
                ))
                .cite("Steve Jobs"),
        )
        .child(label("Citations"))
        .child(
            row()
                .child(Text::new("Spacing follows the scale"))
                .child(Citation::new("cite-1", source.clone(), 1))
                .child(Citation::new("cite-2", source, 2).variant(CitationVariant::Number)),
        )
        .into_any_element()
}
