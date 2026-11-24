//! Text entry examples: the gallery's Inputs page and the doc example of
//! TextInput and TextArea.

use std::time::Duration;

use astryx_primitives::ElementSize;
use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Task, Window, px};

use crate::field::{InputStatus, StatusPlacement};
use crate::icon::Icon;
use crate::input::{InputType, TextArea, TextInput};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

/// Stores the new text and redraws.
fn store(entity: Entity<SharedString>) -> impl Fn(SharedString, &mut Window, &mut App) + 'static {
    move |next, window, cx| {
        entity.update(cx, |v, _| *v = next);
        window.refresh();
    }
}

/// A one-second save, then the new text.
fn slow_save(
    entity: Entity<SharedString>,
) -> impl Fn(SharedString, &mut Window, &mut App) -> Task<()> + 'static {
    move |next, window, cx| {
        let wait = cx.background_executor().timer(Duration::from_secs(1));
        let entity = entity.clone();
        window.spawn(cx, async move |cx| {
            wait.await;
            cx.update(|window, cx| {
                entity.update(cx, |v, _| *v = next);
                window.refresh();
            })
            .ok();
        })
    }
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let text = |key: &'static str, initial: &'static str, window: &mut Window, cx: &mut App| {
        window.use_keyed_state(key, cx, move |_, _| SharedString::from(initial))
    };
    let name = text("name", "", window, cx);
    let email = text("email", "ada@example", window, cx);
    let search = text("search", "astryx", window, cx);
    let password = text("password", "hunter2", window, cx);
    let handle = text("handle", "ada", window, cx);
    let small = text("small", "", window, cx);
    let large = text("large", "", window, cx);
    let entered = text("entered", "", window, cx);
    let bio = text("bio", "", window, cx);
    let notes = text(
        "notes",
        "Shipping notes.
Leave the parcel with the neighbor at number 12.",
        window,
        cx,
    );
    let tweet = text(
        "tweet",
        "Porting Astryx to GPUI, one component at a time",
        window,
        cx,
    );
    let submitted = entered.read(cx).clone();

    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("TextInput"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(px(360.))
                .child(
                    TextInput::new("name", "Full name", name.read(cx).clone())
                        .placeholder("Ada Lovelace")
                        .description("As it appears on your passport.")
                        .required(true)
                        .on_change(store(name.clone()))
                        .on_enter({
                            let (name, entered) = (name.clone(), entered.clone());
                            move |window, cx| {
                                let value = name.read(cx).clone();
                                entered.update(cx, |v, _| *v = value);
                                window.refresh();
                            }
                        }),
                )
                .child(
                    Text::new(if submitted.is_empty() {
                        SharedString::from("Press Enter in Full name to submit it.")
                    } else {
                        SharedString::from(format!("Submitted: {submitted}"))
                    })
                    .text_type(TextType::Supporting),
                )
                .child(
                    TextInput::new("email", "Email", email.read(cx).clone())
                        .input_type(InputType::Email)
                        .status(InputStatus::error("Email must include a domain, like .com"))
                        .on_change(store(email.clone())),
                )
                .child(
                    TextInput::new("search", "Search", search.read(cx).clone())
                        .input_type(InputType::Search)
                        .label_hidden(true)
                        .placeholder("Search components")
                        .start_icon(Icon::new("search"))
                        .clear(true)
                        .on_change(store(search.clone())),
                )
                .child(
                    TextInput::new("password", "Password", password.read(cx).clone())
                        .input_type(InputType::Password)
                        .status(InputStatus::success("Strong password"))
                        .status_placement(StatusPlacement::Detached)
                        .on_change(store(password.clone())),
                )
                .child(
                    TextInput::new("handle", "Username (1 s save)", handle.read(cx).clone())
                        .optional(true)
                        .change_action(slow_save(handle.clone())),
                )
                .child(
                    TextInput::new("workspace", "Workspace", "astryx-rust")
                        .read_only(true)
                        .description("Set by your organization."),
                )
                .child(
                    TextInput::new("seat", "Seat", "Unassigned")
                        .disabled(true)
                        .disabled_message("Seats open after payment."),
                )
                .child(
                    TextInput::new("lookup", "Lookup", "")
                        .loading(true)
                        .placeholder("Loading…"),
                ),
        )
        .child(label("TextArea"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(px(360.))
                .child(
                    TextArea::new("bio", "Bio", bio.read(cx).clone())
                        .placeholder("A few words about you")
                        .description("Shown on your profile.")
                        .on_change(store(bio.clone())),
                )
                .child(
                    TextArea::new("notes", "Delivery notes", notes.read(cx).clone())
                        .rows(4)
                        .size(ElementSize::Lg)
                        .on_change(store(notes.clone())),
                )
                .child(
                    TextArea::new("tweet", "Post", tweet.read(cx).clone())
                        .max_length(50)
                        .status(InputStatus::warning("Posts over 50 characters get cut."))
                        .on_change(store(tweet.clone())),
                )
                .child(
                    TextArea::new("terms", "Terms", "Accepted on 1 March.")
                        .rows(2)
                        .disabled(true),
                ),
        )
        .child(label("Sizes"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    TextInput::new("small", "Small", small.read(cx).clone())
                        .size(ElementSize::Sm)
                        .placeholder("sm")
                        .width(px(160.))
                        .on_change(store(small.clone())),
                )
                .child(
                    TextInput::new("medium", "Medium", "")
                        .placeholder("md")
                        .width(px(160.)),
                )
                .child(
                    TextInput::new("large", "Large", large.read(cx).clone())
                        .size(ElementSize::Lg)
                        .placeholder("lg")
                        .width(px(160.))
                        .on_change(store(large.clone())),
                ),
        )
        .into_any_element()
}
