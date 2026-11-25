//! Text entry examples: the gallery's Inputs page and the doc example of
//! TextInput, TextArea, NumberInput, InputGroup and FormLayout.

use std::time::Duration;

use astryx_primitives::ElementSize;
use gpui::{
    AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Styled, Task, Window, px,
};

use crate::button::{Button, ButtonVariant};
use crate::field::{FormDirection, FormLayout, InputStatus, Optionality, StatusPlacement};
use crate::icon::Icon;
use crate::input::{InputGroup, InputGroupText, InputType, NumberInput, TextArea, TextInput};
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
    let number = |key: &'static str, initial: Option<f64>, window: &mut Window, cx: &mut App| {
        window.use_keyed_state(key, cx, move |_, _| initial)
    };
    let set = |entity: Entity<Option<f64>>| {
        move |next: Option<f64>, window: &mut Window, cx: &mut App| {
            entity.update(cx, |v, _| *v = next);
            window.refresh();
        }
    };
    let quantity = number("quantity", Some(2.), window, cx);
    let opacity = number("opacity", Some(80.), window, cx);
    let price = number("price", Some(1299.5), window, cx);
    let guests = number("guests", None, window, cx);
    let site = text("site", "astryx", window, cx);
    let amount = number("amount", Some(25.), window, cx);
    let coupon = text("coupon", "", window, cx);
    let first = text("first", "Ada", window, cx);
    let last = text("last", "Lovelace", window, cx);
    let display = text("display", "Ada L.", window, cx);
    let email2 = text("email2", "", window, cx);
    let notes2 = text("notes2", "", window, cx);

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
        .child(label("NumberInput"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(px(360.))
                .child(
                    NumberInput::new("quantity", "Quantity", *quantity.read(cx))
                        .min(1.)
                        .max(10.)
                        .integer_only(true)
                        .number_steppers(true)
                        .description("Between 1 and 10. Arrow keys and the wheel step it.")
                        .on_change(set(quantity.clone())),
                )
                .child(
                    NumberInput::new("opacity", "Opacity", *opacity.read(cx))
                        .min(0.)
                        .max(100.)
                        .step(5.)
                        .units("%")
                        .number_steppers(true)
                        .on_change(set(opacity.clone())),
                )
                .child(
                    NumberInput::new("price", "Price", *price.read(cx))
                        .format_value(|v| format!("${v:.2}"))
                        .step(0.5)
                        .clear(true)
                        .on_change(set(price.clone())),
                )
                .child(
                    NumberInput::new("guests", "Guests", *guests.read(cx))
                        .placeholder("How many?")
                        .integer_only(true)
                        .optional(true)
                        .on_change(set(guests.clone())),
                )
                .child(NumberInput::new("rooms", "Rooms", Some(3.)).disabled(true)),
        )
        .child(label("InputGroup"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(px(360.))
                .child(
                    InputGroup::new("site-group", "Website")
                        .description("Your public profile address.")
                        .child(InputGroupText::new("https://"))
                        .child(
                            TextInput::new("site", "Subdomain", site.read(cx).clone())
                                .on_change(store(site.clone())),
                        )
                        .child(InputGroupText::new(".example.com")),
                )
                .child(
                    InputGroup::new("amount-group", "Amount")
                        .child(InputGroupText::new("$"))
                        .child(
                            NumberInput::new("amount", "in USD", *amount.read(cx))
                                .min(0.)
                                .on_change(set(amount.clone())),
                        )
                        .child(InputGroupText::new("USD")),
                )
                .child(
                    InputGroup::new("coupon-group", "Coupon")
                        .status(InputStatus::error("That code has expired."))
                        .child(
                            TextInput::new("coupon", "Code", coupon.read(cx).clone())
                                .placeholder("SPRING25")
                                .on_change(store(coupon.clone())),
                        )
                        .child(Button::new("apply", "Apply").variant(ButtonVariant::Secondary)),
                )
                .child(
                    InputGroup::new("search-group", "Search")
                        .label_hidden(true)
                        .disabled(true)
                        .child(TextInput::new("q", "Query", "").placeholder("Search is offline"))
                        .child(Button::new("go", "Go")),
                ),
        )
        .child(label("FormLayout"))
        .child(
            FormLayout::new("profile")
                .optionality(Optionality::Required)
                .w(px(480.))
                .child(
                    FormLayout::new("name-row")
                        .direction(FormDirection::Horizontal)
                        .child(
                            TextInput::new("first", "First name", first.read(cx).clone())
                                .on_change(store(first.clone())),
                        )
                        .child(
                            TextInput::new("last", "Last name", last.read(cx).clone())
                                .on_change(store(last.clone())),
                        ),
                )
                .child(
                    TextInput::new("email2", "Email", email2.read(cx).clone())
                        .input_type(InputType::Email)
                        .on_change(store(email2.clone())),
                )
                .child(
                    TextArea::new("notes2", "Notes", notes2.read(cx).clone())
                        .optional(true)
                        .rows(2)
                        .on_change(store(notes2.clone())),
                ),
        )
        .child(
            FormLayout::new("settings")
                .direction(FormDirection::HorizontalLabels)
                .w(px(480.))
                .child(
                    TextInput::new("display", "Display name", display.read(cx).clone())
                        .description("Shown on posts and comments.")
                        .on_change(store(display.clone())),
                )
                .child(
                    NumberInput::new("session", "Session timeout", Some(30.))
                        .units("min")
                        .min(5.),
                )
                .child(TextInput::new("org", "Organization", "Astryx").read_only(true)),
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
