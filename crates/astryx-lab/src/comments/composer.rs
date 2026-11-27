//! The composer every Comments surface shares, and the standalone CommentComposer
//! (upstream lab `Comments/CommentComposer.tsx`): a growing input with typeahead
//! triggers and image paste, above attach and emoji buttons and Post.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_components::button::{ButtonVariant, IconButton};
use astryx_components::chat::{
    ChatComposer, ChatComposerElevation, ChatComposerInput, ChatComposerInputHandle,
    ChatComposerTrigger, ChatDensity, ChatSendButton, ChatSendButtonSize,
};
use astryx_components::containers::{Card, CardVariant, Elevation};
use astryx_components::icon::{Icon, IconColor};
use astryx_components::input::PickedFile;
use astryx_components::layout::SpacingStep;
use astryx_components::overlay::Popover;
use astryx_components::people::{Avatar, AvatarShape, AvatarSize};
use astryx_components::search::{SearchItem, Searchable, static_source};
use astryx_components::text::{Text, TextColor, TextType, TextWeight};
use astryx_i18n::Arg;
use astryx_primitives::{ElementSize, LayerAlign, Placement};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, PathPromptOptions, SharedString, StyleRefinement, Task, Window,
    div, px, relative,
};

use super::markdown::agent_token;
use super::parts::{ErrorBannerContent, action_link, error_banner, quoted_passage, separator};
use super::types::{
    ActionFn, CommentAgentCommand, CommentAnchor, CommentComposerContext,
    CommentComposerRenderProps, CommentHighlightColor, CommentUploadedImage, DraftFn,
    RenderComposerFn,
};
use super::utils::{append_draft_block, append_draft_inline, images_to_markdown};

/// A composer's uploader: the host's `onUploadImages` bound to the composer's context.
pub(crate) type BoundUploader = Rc<
    dyn Fn(Vec<PickedFile>, &mut Window, &mut App) -> Task<Result<Vec<CommentUploadedImage>, ()>>,
>;
/// Reports an upload in flight.
pub(crate) type PendingFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `useAgentCommandTrigger`: the `/` menu of the agents a comment can
/// address, or `None` for none.
pub(crate) fn agent_command_trigger(
    agents: &[CommentAgentCommand],
    cx: &App,
) -> Option<ChatComposerTrigger> {
    if agents.is_empty() {
        return None;
    }
    let i18n = astryx_i18n::current(cx);
    let items: Vec<SearchItem> = agents
        .iter()
        .map(|agent| {
            SearchItem::new(agent.command.clone(), format!("/{}", agent.command))
                .keywords([agent.command.clone(), agent.name.clone()])
                .data(agent.clone())
        })
        .collect();
    let list: Rc<Vec<CommentAgentCommand>> = Rc::new(agents.to_vec());
    let pick = list.clone();
    Some(
        ChatComposerTrigger::new('/', static_source(items), move |item| {
            let agent = pick
                .iter()
                .find(|a| a.command.as_ref() == item.id().as_ref())
                .cloned();
            match agent {
                Some(agent) => agent_token(&agent).into(),
                None => SharedString::from(format!("/{}", item.id())).into(),
            }
        })
        .render_item(move |item, _, _| {
            let Some(agent) = list
                .iter()
                .find(|a| a.command.as_ref() == item.id().as_ref())
            else {
                return item.label().into_any_element();
            };
            let mut avatar = Avatar::new(ElementId::Name(
                format!("agent-row-{}", agent.command).into(),
            ))
            .name(agent.name.clone())
            .size(AvatarSize::Sm)
            .shape(AvatarShape::Rounded);
            if let Some(logo) = &agent.logo_url {
                avatar = avatar.src(logo.clone());
            }
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(avatar)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            Text::new(format!("/{} <prompt>", agent.command))
                                .weight(TextWeight::Semibold),
                        )
                        .child(
                            Text::new(format!("{} \u{b7} {}", agent.name, agent.description))
                                .text_type(TextType::Supporting)
                                .color(TextColor::Secondary),
                        ),
                )
                .into_any_element()
        })
        .empty_search_text(i18n.t("@astryx.commentComposer.noAgents"))
        .menu_label(i18n.t("@astryx.commentComposer.agents")),
    )
}

/// What the composer keeps across frames.
struct ComposerState {
    action_error: Option<ErrorBannerContent>,
    /// Counts action failures so each mounts its own Banner and is announced again.
    action_error_serial: u64,
    is_action_pending: bool,
    is_picker_open: bool,
    /// The last `focus_key` focused: focus lands once per key.
    focused_key: Option<SharedString>,
    /// The handlers' draft, the committed value.
    latest_value: SharedString,
    latest_submitting: bool,
    mounted: bool,
}

/// Upstream `CommentComposerBaseProps`: what the family's threads pass to their
/// composers.
pub(crate) struct ComposerBase {
    pub id: ElementId,
    pub value: SharedString,
    pub on_change: DraftFn,
    pub on_submit: DraftFn,
    pub placeholder: SharedString,
    pub label: Option<SharedString>,
    pub is_submitting: bool,
    pub error: Option<SharedString>,
    /// Adds a Retry action to `error`'s Banner.
    pub on_retry_error: Option<ActionFn>,
    pub on_cancel: Option<ActionFn>,
    pub context_label: Option<SharedString>,
    pub context: CommentComposerContext,
    pub anchor: Option<CommentAnchor>,
    pub on_anchor_dismiss: Option<ActionFn>,
    pub highlight_color: CommentHighlightColor,
    pub on_upload_images: Option<BoundUploader>,
    pub triggers: Vec<ChatComposerTrigger>,
    pub agent_commands: Vec<CommentAgentCommand>,
    pub has_emoji_picker: bool,
    pub controls: Option<AnyElement>,
    /// Focuses the input whenever it changes, e.g. when Reply opens it.
    pub focus_key: Option<SharedString>,
    /// Reports an upload in flight, so a thread can hold its other actions.
    pub on_action_pending_change: Option<PendingFn>,
    pub render_composer: Option<RenderComposerFn>,
    pub style: StyleRefinement,
}

impl ComposerBase {
    /// A composer with its draft and callbacks.
    pub fn new(
        id: impl Into<ElementId>,
        value: SharedString,
        on_change: DraftFn,
        on_submit: DraftFn,
        placeholder: SharedString,
        context: CommentComposerContext,
    ) -> Self {
        ComposerBase {
            id: id.into(),
            value,
            on_change,
            on_submit,
            placeholder,
            label: None,
            is_submitting: false,
            error: None,
            on_retry_error: None,
            on_cancel: None,
            context_label: None,
            context,
            anchor: None,
            on_anchor_dismiss: None,
            highlight_color: CommentHighlightColor::Yellow,
            on_upload_images: None,
            triggers: Vec::new(),
            agent_commands: Vec::new(),
            has_emoji_picker: true,
            controls: None,
            focus_key: None,
            on_action_pending_change: None,
            render_composer: None,
            style: StyleRefinement::default(),
        }
    }

    /// The input's id, for focus.
    pub fn input_id(id: &ElementId) -> ElementId {
        ElementId::Name(format!("{id}-input").into())
    }
}

/// The shared behavior the composer's closures read.
#[derive(Clone)]
struct Shared {
    state: gpui::Entity<ComposerState>,
    on_change: DraftFn,
    on_upload: Option<BoundUploader>,
    on_pending: Option<PendingFn>,
}

impl Shared {
    fn update(&self, cx: &mut App, f: impl FnOnce(&mut ComposerState)) {
        self.state.update(cx, |s, cx| {
            f(s);
            cx.notify();
        });
    }

    fn report_error(&self, content: ErrorBannerContent, cx: &mut App) {
        self.update(cx, |s| {
            s.action_error = Some(content);
            s.action_error_serial += 1;
        });
    }

    fn set_pending(&self, pending: bool, window: &mut Window, cx: &mut App) {
        if self.state.read(cx).is_action_pending == pending {
            return;
        }
        self.update(cx, |s| s.is_action_pending = pending);
        if let Some(f) = &self.on_pending {
            f(pending, window, cx);
        }
    }

    fn busy(&self, cx: &App) -> bool {
        let s = self.state.read(cx);
        s.latest_submitting || s.is_action_pending
    }

    /// Upstream `beginImageUpload`: the picker, a paste and a drop all arrive here. A
    /// busy composer starts no upload.
    fn begin_upload(&self, files: Vec<PickedFile>, window: &mut Window, cx: &mut App) {
        let Some(upload) = self.on_upload.clone() else {
            return;
        };
        if files.is_empty() || self.busy(cx) {
            return;
        }
        let i18n = astryx_i18n::current(cx);
        let images: Vec<PickedFile> = files
            .iter()
            .filter(|f| f.mime.starts_with("image/"))
            .cloned()
            .collect();
        if images.len() != files.len() {
            let me = self.clone();
            self.report_error(
                ErrorBannerContent {
                    title: i18n.t("@astryx.commentComposer.imagesOnly"),
                    action_label: Some(i18n.t("@astryx.commentComposer.chooseImages")),
                    on_action: Some(Rc::new(move |window, cx| {
                        if !me.busy(cx) {
                            me.choose_images(window, cx);
                        }
                    })),
                    ..ErrorBannerContent::default()
                },
                cx,
            );
            return;
        }
        self.set_pending(true, window, cx);
        self.update(cx, |s| s.action_error = None);
        let task = upload(images.clone(), window, cx);
        let me = self.clone();
        window
            .spawn(cx, async move |cx| {
                let result = task.await;
                let _ = cx.update(|window, cx| {
                    if !me.state.read(cx).mounted {
                        return;
                    }
                    let i18n = astryx_i18n::current(cx);
                    match result {
                        Ok(uploaded) => {
                            let markdown = images_to_markdown(
                                &uploaded,
                                &i18n.t("@astryx.commentComposer.imageAlt"),
                            );
                            if !markdown.is_empty() {
                                let latest = me.state.read(cx).latest_value.clone();
                                (me.on_change)(
                                    append_draft_block(&latest, &markdown).into(),
                                    window,
                                    cx,
                                );
                            }
                        }
                        Err(()) => {
                            let retry = me.clone();
                            let files = images.clone();
                            me.report_error(
                                ErrorBannerContent {
                                    title: i18n.t("@astryx.commentComposer.uploadFailed"),
                                    action_label: Some(i18n.t("@astryx.comments.retry")),
                                    on_action: Some(Rc::new(move |window, cx| {
                                        retry.begin_upload(files.clone(), window, cx)
                                    })),
                                    ..ErrorBannerContent::default()
                                },
                                cx,
                            );
                        }
                    }
                    me.set_pending(false, window, cx);
                });
            })
            .detach();
    }

    /// The attach button's chooser (upstream's hidden `<input type="file">`).
    fn choose_images(&self, window: &mut Window, cx: &mut App) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let me = self.clone();
        window
            .spawn(cx, async move |cx| {
                let Ok(Ok(Some(paths))) = paths.await else {
                    return;
                };
                let files: Vec<PickedFile> = paths.into_iter().map(PickedFile::from_path).collect();
                let _ = cx.update(|window, cx| {
                    if !me.busy(cx) {
                        me.begin_upload(files, window, cx);
                    }
                });
            })
            .detach();
    }
}

impl IntoElement for ComposerBase {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        ComposerElement(RefCell::new(Some(self))).into_any_element()
    }
}

/// The composer as an element.
#[derive(IntoElement)]
struct ComposerElement(RefCell<Option<ComposerBase>>);

impl RenderOnce for ComposerElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let props = self.0.borrow_mut().take().expect("rendered once");
        render_composer(props, window, cx)
    }
}

fn render_composer(props: ComposerBase, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let i18n = astryx_i18n::current(cx);
    let id = props.id.clone();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-composer").into()),
        cx,
        |_, _| ComposerState {
            action_error: None,
            action_error_serial: 0,
            is_action_pending: false,
            is_picker_open: false,
            focused_key: None,
            latest_value: SharedString::default(),
            latest_submitting: false,
            mounted: true,
        },
    );
    let handle = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-composer-input").into()),
            cx,
            |_, _| ChatComposerInputHandle::new(),
        )
        .read(cx)
        .clone();
    state.update(cx, |s, _| {
        s.latest_value = props.value.clone();
        s.latest_submitting = props.is_submitting;
        s.mounted = true;
    });
    let shared = Shared {
        state: state.clone(),
        on_change: props.on_change.clone(),
        on_upload: props.on_upload_images.clone(),
        on_pending: props.on_action_pending_change.clone(),
    };
    // Focus lands once per `focus_key`.
    match &props.focus_key {
        None => state.update(cx, |s, _| s.focused_key = None),
        Some(key) if state.read(cx).focused_key.as_ref() != Some(key) => {
            let key = key.clone();
            state.update(cx, |s, _| s.focused_key = Some(key));
            let h = handle.clone();
            window.defer(cx, move |window, cx| h.focus(window, cx));
        }
        _ => {}
    }
    let s = state.read(cx);
    let is_busy = props.is_submitting || s.is_action_pending;
    let action_error = s.action_error.clone();
    let serial = s.action_error_serial;
    let picker_open = s.is_picker_open;
    // Busy states never disable the input; duplicate submits are refused here.
    let submit_input: DraftFn = {
        let on_submit = props.on_submit.clone();
        let shared = shared.clone();
        Rc::new(
            move |value: SharedString, window: &mut Window, cx: &mut App| {
                if shared.busy(cx) || astryx_theme::authoring::js::js_trim(&value).is_empty() {
                    return;
                }
                on_submit(value, window, cx);
            },
        )
    };
    let error = match &props.error {
        Some(error) => Some((
            "error".to_string(),
            ErrorBannerContent {
                title: error.clone(),
                action_label: props
                    .on_retry_error
                    .as_ref()
                    .map(|_| i18n.t("@astryx.comments.retry")),
                on_action: props.on_retry_error.clone(),
                ..ErrorBannerContent::default()
            },
        )),
        None => action_error.map(|c| (format!("action-{serial}"), c)),
    };
    let composer: AnyElement = if let Some(render) = &props.render_composer {
        render(
            CommentComposerRenderProps {
                value: props.value.clone(),
                on_change: props.on_change.clone(),
                on_submit: props.on_submit.clone(),
                placeholder: props.placeholder.clone(),
                is_submitting: props.is_submitting,
                error: props.error.clone(),
                context_label: props.context_label.clone(),
                context: props.context.clone(),
                on_cancel: props.on_cancel.clone(),
            },
            window,
            cx,
        )
    } else {
        let banner = error.map(|(key, content)| {
            let focus = handle.clone();
            error_banner(
                ElementId::Name(format!("{id}-composer-error-{key}").into()),
                content,
                // The Banner unmounts once the retry starts; keep focus in the input.
                Some(Rc::new(move |window, cx| focus.focus(window, cx))),
                window,
                cx,
            )
        });
        let quote = props.anchor.as_ref().map(|anchor| {
            let dismiss = props.on_anchor_dismiss.clone().map(|f| {
                IconButton::new(
                    ElementId::Name(format!("{id}-remove-quote").into()),
                    i18n.t("@astryx.commentComposer.removeQuote"),
                    Icon::new("close"),
                )
                .tooltip(i18n.t("@astryx.commentComposer.removeQuote"))
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .on_click(move |_, window, cx| f(window, cx))
            });
            Card::new()
                .variant(CardVariant::Muted)
                .elevation(Elevation::None)
                .padding(SpacingStep::S2)
                .width(relative(1.))
                .rounded(t.radius.inner)
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(t.spacing.s1)
                        .w_full()
                        .debug_selector(|| "composer-anchor-quote".into())
                        .child(div().flex_1().min_w_0().child(quoted_passage(
                            &anchor.quote,
                            Some(props.highlight_color),
                            window,
                            cx,
                        )))
                        .children(dismiss.map(|d| {
                            let s = format!("{id}-remove-quote");
                            div().debug_selector(move || s).child(d)
                        })),
                )
        });
        let mut triggers = props.triggers.clone();
        if let Some(agents) = agent_command_trigger(&props.agent_commands, cx) {
            triggers.push(agents);
        }
        let mut input = ChatComposerInput::new(ComposerBase::input_id(&id))
            .handle(handle.clone())
            .value(props.value.clone())
            .placeholder(props.placeholder.clone())
            .label(
                props
                    .label
                    .clone()
                    .unwrap_or_else(|| props.placeholder.clone()),
            )
            .history(false)
            .no_paste_as_token()
            .max_rows(5usize)
            .triggers(triggers)
            .on_change({
                let f = props.on_change.clone();
                move |v, w, cx| f(v, w, cx)
            })
            .on_submit({
                let f = submit_input.clone();
                move |v, w, cx| f(v, w, cx)
            });
        if props.on_upload_images.is_some() {
            let s = shared.clone();
            input = input.on_files(move |files, window, cx| s.begin_upload(files, window, cx));
        }
        let attach = props.on_upload_images.as_ref().map(|_| {
            let s = shared.clone();
            IconButton::new(
                ElementId::Name(format!("{id}-attach").into()),
                i18n.t("@astryx.commentComposer.attachImages"),
                super::icons::CommentIcon::new(super::icons::CommentIconName::Attach)
                    .color(IconColor::Inherit),
            )
            .tooltip(i18n.t("@astryx.commentComposer.attachImages"))
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .disabled(is_busy)
            .on_click(move |_, window, cx| s.choose_images(window, cx))
        });
        let emoji = props.has_emoji_picker.then(|| {
            let (open_state, pick_state) = (state.clone(), state.clone());
            let (focus, change) = (handle.clone(), props.on_change.clone());
            let trigger_state = state.clone();
            let picker = picker_open.then(|| {
                crate::emoji_picker::EmojiPicker::new(ElementId::Name(
                    format!("{id}-emoji-picker").into(),
                ))
                .on_select(move |emoji, _, window, cx| {
                    let latest = pick_state.read(cx).latest_value.clone();
                    change(
                        append_draft_inline(&latest, emoji.as_ref()).into(),
                        window,
                        cx,
                    );
                    pick_state.update(cx, |s, cx| {
                        s.is_picker_open = false;
                        cx.notify();
                    });
                    focus.focus(window, cx);
                })
            });
            let mut popover = Popover::new(
                ElementId::Name(format!("{id}-emoji").into()),
                i18n.t("@astryx.commentComposer.addEmoji"),
            )
            .open(picker_open)
            .placement(Placement::Above)
            .alignment(LayerAlign::Start)
            .width(t.spacing.s8.to_pixels(window.rem_size()) * 11.)
            .padding(SpacingStep::S0)
            .on_open_change(move |open, _, cx| {
                open_state.update(cx, |s, cx| {
                    s.is_picker_open = open;
                    cx.notify();
                })
            })
            .child(
                IconButton::new(
                    ElementId::Name(format!("{id}-emoji-button").into()),
                    i18n.t("@astryx.commentComposer.addEmoji"),
                    super::icons::CommentIcon::new(super::icons::CommentIconName::Emoji)
                        .color(IconColor::Inherit),
                )
                .tooltip(i18n.t("@astryx.commentComposer.addEmoji"))
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .disabled(is_busy)
                .on_click(move |_, _, cx| {
                    trigger_state.update(cx, |s, cx| {
                        if !(s.latest_submitting || s.is_action_pending) {
                            s.is_picker_open = !s.is_picker_open;
                        }
                        cx.notify();
                    })
                }),
            );
            if let Some(picker) = picker {
                popover = popover.content(picker);
            }
            popover
        });
        let footer = div()
            .flex()
            .items_center()
            .gap(t.spacing.s0_5)
            .children(attach.map(|b| {
                let s = format!("{id}-attach");
                div().debug_selector(move || s).child(b)
            }))
            .children(emoji.map(|e| {
                let s = format!("{id}-emoji-button");
                div().debug_selector(move || s).child(e)
            }))
            .children(props.controls);
        let send = {
            let (focus, s, submit) = (handle.clone(), shared.clone(), submit_input.clone());
            ChatSendButton::new(ElementId::Name(format!("{id}-post").into()))
                .size(ChatSendButtonSize::Sm)
                .disabled(astryx_theme::authoring::js::js_trim(&props.value).is_empty())
                .on_send(move |window, cx| {
                    // Post disables once the draft clears; keep focus in the input.
                    focus.focus(window, cx);
                    let latest = s.state.read(cx).latest_value.clone();
                    submit(latest, window, cx);
                })
        };
        let selector = format!("{id}-post");
        let chat = ChatComposer::new(ElementId::Name(format!("{id}-chat").into()), {
            let f = submit_input.clone();
            move |v, w, cx| f(v, w, cx)
        })
        .value(props.value.clone())
        .placeholder(props.placeholder.clone())
        .density(ChatDensity::Compact)
        .elevation(ChatComposerElevation::None)
        .input(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s1)
                .w_full()
                .children(quote)
                .child(input),
        )
        .footer_actions(footer)
        .send_button(div().debug_selector(move || selector).child(send));
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.s1)
            .w_full()
            .children(banner)
            .child(chat)
            .into_any_element()
    };
    let context_row = matches!(props.context, CommentComposerContext::Edit { .. })
        .then_some(props.context_label.clone())
        .flatten()
        .map(|label| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(t.spacing.s1)
                .child(
                    Text::new(label)
                        .text_type(TextType::Supporting)
                        .color(TextColor::Secondary),
                )
                .children(props.on_cancel.clone().map(|cancel| {
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s1)
                        .child(separator(TextType::Supporting))
                        .child(
                            div()
                                .debug_selector({
                                    let s = format!("{id}-cancel");
                                    move || s
                                })
                                .child(action_link(
                                    ElementId::Name(format!("{id}-cancel").into()),
                                    i18n.t("@astryx.commentComposer.cancel"),
                                    None,
                                    is_busy,
                                    cancel,
                                )),
                        )
                }))
        });
    let focus = handle.clone();
    let mut root = div()
        .id(ElementId::Name(format!("{id}-root").into()))
        .flex()
        .flex_col()
        .gap(t.spacing.s2)
        .w_full()
        .debug_selector({
            let s = id.to_string();
            move || s
        })
        .on_click(move |_, window, cx| {
            // The composer keeps its clicks: in a thread, a press on its empty space
            // would otherwise reach the thread and collapse it. A press beside its
            // controls focuses the input.
            cx.stop_propagation();
            if window.focused(cx).is_none() {
                focus.focus(window, cx);
            }
        });
    theme.refine(root.style(), "comment-composer", &[], None);
    root.style().refine(&props.style);
    let _ = Arg::from("");
    root.children(context_row)
        .child(composer)
        .into_any_element()
}

/// Upstream lab `CommentComposer`: a comment composer, controlled: the host owns the
/// draft and clears it once the comment posts.
pub struct CommentComposer {
    base: ComposerBase,
    placeholder_set: bool,
}

impl CommentComposer {
    /// Upstream `value`, `onChange` and `onSubmit` (Enter, or the Post button; empty
    /// drafts never post).
    pub fn new(
        id: impl Into<ElementId>,
        value: impl Into<SharedString>,
        on_change: impl Fn(SharedString, &mut Window, &mut App) + 'static,
        on_submit: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        CommentComposer {
            base: ComposerBase::new(
                id,
                value.into(),
                Rc::new(on_change),
                Rc::new(on_submit),
                SharedString::default(),
                CommentComposerContext::Comment { anchor: None },
            ),
            placeholder_set: false,
        }
    }

    /// Upstream `placeholder` (the localized "Write a comment…").
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.base.placeholder = placeholder.into();
        self.placeholder_set = true;
        self
    }

    /// Upstream `label`: the input's accessible name (the placeholder by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.base.label = Some(label.into());
        self
    }

    /// Upstream `isSubmitting`: holds Post and the attach and emoji buttons.
    pub fn submitting(mut self, submitting: bool) -> Self {
        self.base.is_submitting = submitting;
        self
    }

    /// Upstream `error`: an error Banner above the composer.
    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.base.error = Some(error.into());
        self
    }

    /// Upstream `onRetry`: Retry on the error Banner.
    pub fn on_retry(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.base.on_retry_error = Some(Rc::new(f));
        self
    }

    /// Upstream `anchor`: the passage a new inline comment attaches to.
    pub fn anchor(mut self, anchor: CommentAnchor) -> Self {
        self.base.context = CommentComposerContext::Comment {
            anchor: Some(anchor.clone()),
        };
        self.base.anchor = Some(anchor);
        self
    }

    /// Upstream `onAnchorDismiss`: Remove quoted text on the passage.
    pub fn on_anchor_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.base.on_anchor_dismiss = Some(Rc::new(f));
        self
    }

    /// Upstream `highlightColor` (yellow by default).
    pub fn highlight_color(mut self, color: CommentHighlightColor) -> Self {
        self.base.highlight_color = color;
        self
    }

    /// Upstream `onUploadImages`: uploads attached, pasted or dropped images, and adds
    /// the attach button.
    pub fn on_upload_images(
        mut self,
        f: impl Fn(
            Vec<PickedFile>,
            &mut Window,
            &mut App,
        ) -> Task<Result<Vec<CommentUploadedImage>, ()>>
        + 'static,
    ) -> Self {
        self.base.on_upload_images = Some(Rc::new(f));
        self
    }

    /// Upstream `triggers`: typeahead triggers, such as an `@` people search.
    pub fn triggers(mut self, triggers: impl IntoIterator<Item = ChatComposerTrigger>) -> Self {
        self.base.triggers = triggers.into_iter().collect();
        self
    }

    /// Upstream `agentCommands`: agents the comment can address with `/`.
    pub fn agent_commands(mut self, agents: impl IntoIterator<Item = CommentAgentCommand>) -> Self {
        self.base.agent_commands = agents.into_iter().collect();
        self
    }

    /// Upstream `hasEmojiPicker` (on by default).
    pub fn emoji_picker(mut self, on: bool) -> Self {
        self.base.has_emoji_picker = on;
        self
    }

    /// Upstream `controls`: product-owned controls after the attach and emoji buttons.
    pub fn controls(mut self, controls: impl IntoElement) -> Self {
        self.base.controls = Some(controls.into_any_element());
        self
    }
}

impl Styled for CommentComposer {
    /// The root's style (upstream `xstyle`, `className`, `style`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.base.style
    }
}

impl IntoElement for CommentComposer {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        StandaloneComposer(RefCell::new(Some(self))).into_any_element()
    }
}

#[derive(IntoElement)]
struct StandaloneComposer(RefCell<Option<CommentComposer>>);

impl RenderOnce for StandaloneComposer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut composer = self.0.borrow_mut().take().expect("rendered once");
        if !composer.placeholder_set {
            composer.base.placeholder =
                astryx_i18n::current(cx).t("@astryx.commentComposer.placeholder");
        }
        composer.base
    }
}
