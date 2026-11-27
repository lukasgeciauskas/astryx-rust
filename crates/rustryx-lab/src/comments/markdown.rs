//! A comment body (upstream lab `Comments/CommentMarkdown.tsx`): compact Markdown whose
//! images open in a Lightbox, with an agent's `/command` shown as its chip.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, ElementId, KeyDownEvent, SharedString, Window, div, img, px, relative};
use rustryx_components::chat::{ChatComposerToken, ChatTokenizedText};
use rustryx_components::markdown::{
    Markdown, MarkdownAutolink, MarkdownComponents, MarkdownDensity, MarkdownInlinePlugin,
};
use rustryx_components::overlay::{Lightbox, LightboxMedia};
use rustryx_components::people::{Avatar, AvatarShape, AvatarSize};
use rustryx_components::status::BadgeVariant;
use rustryx_markdown::plugins::MarkdownPlugin;

use super::types::CommentAgentCommand;

/// Upstream `agentToken`: an agent's chip: what the composer's `/` menu inserts, and
/// what a posted comment shows in place of the command.
pub fn agent_token(agent: &CommentAgentCommand) -> ChatComposerToken {
    let (name, logo) = (agent.name.clone(), agent.logo_url.clone());
    ChatComposerToken::badge(format!("/{}", agent.command), agent.name.clone())
        .variant(BadgeVariant::Info)
        .icon(move |_, _| {
            let mut avatar = Avatar::new(ElementId::Name(format!("agent-chip-{name}").into()))
                .name(name.clone())
                .size(AvatarSize::Xsm)
                .shape(AvatarShape::Rounded);
            if let Some(logo) = &logo {
                avatar = avatar.src(logo.clone());
            }
            avatar.into_any_element()
        })
}

/// Upstream `escapeRegExp`.
fn escape_regexp(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if ".*+?^${}()|[]\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Upstream `agentCommandPlugin`: shows `/<command>` in a posted comment as the
/// agent's chip.
fn agent_command_plugin(agents: &[CommentAgentCommand]) -> Option<MarkdownInlinePlugin> {
    let alternation = agents
        .iter()
        .map(|a| escape_regexp(&a.command))
        .collect::<Vec<_>>()
        .join("|");
    let pattern = fancy_regex::Regex::new(&format!("(^|\\s)/({alternation})(?=\\s|$)")).ok()?;
    let agents: Rc<Vec<CommentAgentCommand>> = Rc::new(agents.to_vec());
    Some(MarkdownInlinePlugin::new(pattern, move |captures, _, _| {
        let command = captures.get(2).map(|m| m.as_str()).unwrap_or("");
        let Some(agent) = agents.iter().find(|a| a.command.as_ref() == command) else {
            return captures
                .get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
                .into_any_element();
        };
        let token = agent_token(agent);
        let lead = captures
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let selector = format!("agent-command-{}", agent.command);
        // Set in the line as the composer sets it: centered on the text.
        div()
            .flex()
            .items_center()
            .debug_selector(move || selector)
            .child(lead)
            .child(ChatTokenizedText::new(token.value().clone()).tokens([token]))
            .into_any_element()
    }))
}

/// Upstream `CommentMarkdown`: a comment body, compact Markdown whose images open in a
/// Lightbox.
#[derive(IntoElement)]
pub struct CommentMarkdown {
    id: ElementId,
    body: SharedString,
    agents: Vec<CommentAgentCommand>,
    plugins: Vec<MarkdownPlugin>,
    inline_plugins: Vec<MarkdownInlinePlugin>,
}

impl CommentMarkdown {
    /// A body that shows `agents`' commands as chips.
    pub fn new(
        id: impl Into<ElementId>,
        body: impl Into<SharedString>,
        agents: Vec<CommentAgentCommand>,
    ) -> Self {
        CommentMarkdown {
            id: id.into(),
            body: body.into(),
            agents,
            plugins: Vec::new(),
            inline_plugins: Vec::new(),
        }
    }

    /// Upstream `plugins`.
    pub fn plugins(mut self, plugins: Vec<MarkdownPlugin>) -> Self {
        self.plugins = plugins;
        self
    }

    /// Upstream `inlinePlugins`.
    pub fn inline_plugins(mut self, plugins: Vec<MarkdownInlinePlugin>) -> Self {
        self.inline_plugins = plugins;
        self
    }
}

impl RenderOnce for CommentMarkdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let opened = window.use_keyed_state(
            ElementId::Name(format!("{}-lightbox", self.id).into()),
            cx,
            |_, _| None::<(SharedString, SharedString)>,
        );
        let mut inline = self.inline_plugins;
        if !self.agents.is_empty()
            && let Some(plugin) = agent_command_plugin(&self.agents)
        {
            inline.push(plugin);
        }
        // One Lightbox per body, opened at whichever of its images is pressed.
        let open_state = opened.clone();
        let margin = t.spacing.s2;
        let components = MarkdownComponents::new().image(move |props, _, _| {
            let (src, alt) = (props.src.clone(), props.alt.clone());
            let (on_click, on_key) = (open_state.clone(), open_state.clone());
            let (click_media, key_media) = ((src.clone(), alt.clone()), (src.clone(), alt.clone()));
            div()
                .id(ElementId::Name(format!("comment-image-{src}").into()))
                .debug_selector(|| "comment-image".into())
                .my(margin)
                .max_w_full()
                .cursor(gpui::CursorStyle::PointingHand)
                .role(gpui::Role::Button)
                .aria_label(alt.clone())
                .on_click(move |_, _, cx| {
                    // An image is its own control, not a press on the thread.
                    cx.stop_propagation();
                    on_click.update(cx, |o, cx| {
                        *o = Some(click_media.clone());
                        cx.notify();
                    });
                })
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        on_key.update(cx, |o, cx| {
                            *o = Some(key_media.clone());
                            cx.notify();
                        });
                    }
                })
                .child(img(src).max_w(relative(1.)).min_h(px(1.)))
                .into_any_element()
        });
        let markdown = Markdown::new(self.id.clone(), self.body)
            .density(MarkdownDensity::Compact)
            .plugins(self.plugins)
            .inline_plugins(inline)
            .autolink(MarkdownAutolink::Gfm)
            .components(components);
        let lightbox = opened.read(cx).clone().map(|(src, alt)| {
            let close = opened.clone();
            Lightbox::new(
                ElementId::Name(format!("{}-lightbox-view", self.id).into()),
                true,
                [LightboxMedia::image(src, alt)],
            )
            .has_zoom(true)
            .on_open_change(move |open, _, cx| {
                if !open {
                    close.update(cx, |o, cx| {
                        *o = None;
                        cx.notify();
                    });
                }
            })
        });
        div().w_full().child(markdown).children(lightbox)
    }
}
