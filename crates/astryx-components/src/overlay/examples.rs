//! Overlay examples: the gallery's Overlays page and the doc example of
//! the overlays and menus.

use astryx_primitives::{LayerAlign, Placement};
use gpui::{
    AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div, prelude::*, px,
};

use crate::button::{Button, ButtonVariant, IconButton};
use crate::choice::Switch;
use crate::field::InputStatus;
use crate::icon::Icon;
use crate::input::TextInput;
use crate::layout::{SpacingStep, Stack};
use crate::overlay::{
    AlertDialog, BottomSheet, Dialog, DialogHeader, DialogPurpose, Drawer, DrawerHeader, HoverCard,
    InfoTip, Overlay, OverlayPosition, OverlayShowOn, Popover, PopoverPresentation, PopoverRole,
    SheetHeight, SnapPoint, ToastOptions, ToastType, Tooltip, show_toast,
};
use crate::overlay::{
    CommandPalette, ContextMenu, DropdownMenu, LightboxMedia, MenuCheckboxItem, MenuEntry,
    MenuItem, MenuRadioGroup, MenuRadioItem, MoreMenu, use_lightbox,
};
use crate::search::{SearchItem, static_source};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let archived = window.use_keyed_state("popover-archived", cx, |_, _| false);
    let shown_archived = *archived.read(cx);
    let rename_open = window.use_keyed_state("dialog-rename-open", cx, |_, _| false);
    let delete_open = window.use_keyed_state("dialog-delete-open", cx, |_, _| false);
    let toggle = |state: &gpui::Entity<bool>, open: bool| {
        let state = state.clone();
        move |window: &mut Window, cx: &mut App| {
            state.update(cx, |v, _| *v = open);
            window.refresh();
        }
    };
    let (open_rename, close_rename) = (toggle(&rename_open, true), toggle(&rename_open, false));
    let (open_delete, close_delete) = (toggle(&delete_open, true), toggle(&delete_open, false));
    let close_rename_header = close_rename.clone();
    let drawer_open = window.use_keyed_state("drawer-open", cx, |_, _| false);
    let sheet_open = window.use_keyed_state("sheet-open", cx, |_, _| false);
    let (open_drawer, close_drawer) = (toggle(&drawer_open, true), toggle(&drawer_open, false));
    let (open_sheet, close_sheet) = (toggle(&sheet_open, true), toggle(&sheet_open, false));
    let close_drawer_header = close_drawer.clone();
    let close_rename_cancel = close_rename.clone();
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let placed = |id: &'static str, placement: Placement, text: &'static str| {
        Tooltip::new(id, text)
            .placement(placement)
            .child(Button::new(format!("{id}-button"), text).variant(ButtonVariant::Secondary))
    };

    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("Tooltip"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Tooltip::new("open", "Shown when the page opens")
                        .default_open(true)
                        .child(Button::new("open-button", "Default open")),
                )
                .child(placed("above", Placement::Above, "Above"))
                .child(placed("below", Placement::Below, "Below"))
                .child(placed("start", Placement::Start, "Start"))
                .child(placed("end", Placement::End, "End")),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    IconButton::new("copy-link", "Copy link", Icon::new("copy"))
                        .variant(ButtonVariant::Ghost)
                        .tooltip("Copy a link to this page"),
                )
                .child(
                    Tooltip::new("aligned", "Aligned to the start edge")
                        .alignment(LayerAlign::Start)
                        .child(Button::new("aligned-button", "Start aligned")),
                )
                .child(
                    Text::new("Times are shown in ")
                        .text_type(TextType::Body)
                        .child(Tooltip::new("utc", "Coordinated Universal Time").text("UTC")),
                ),
        )
        .child(label("Popover"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Popover::new("filters", "Filters")
                        .content(
                            Stack::vertical()
                                .gap(SpacingStep::S3)
                                .width(gpui::px(240.))
                                .child(Text::new("Filters").text_type(TextType::Label))
                                .child(
                                    Switch::new("archived", "Show archived", shown_archived)
                                        .on_change({
                                            let archived = archived.clone();
                                            move |on, window, cx| {
                                                archived.update(cx, |v, _| *v = on);
                                                window.refresh();
                                            }
                                        }),
                                )
                                .child(TextInput::new("owner", "Owner", "")),
                        )
                        .child(Button::new("filters-button", "Filters").variant(ButtonVariant::Secondary)),
                )
                .child(
                    Popover::new("about", "About this report")
                        .placement(Placement::End)
                        .content(
                            Text::new(
                                "Totals refresh every hour. Numbers for today are estimates until midnight.",
                            )
                            .text_type(TextType::Body),
                        )
                        .width(gpui::px(260.))
                        .child(Button::new("about-button", "About").variant(ButtonVariant::Ghost)),
                )
                .child(
                    Popover::new("actions", "")
                        .role(PopoverRole::None)
                        .padding(SpacingStep::S1)
                        .content(
                            Stack::vertical()
                                .child(Button::new("rename", "Rename").variant(ButtonVariant::Ghost))
                                .child(Button::new("duplicate", "Duplicate").variant(ButtonVariant::Ghost)),
                        )
                        .child(Button::new("actions-button", "Actions").variant(ButtonVariant::Secondary)),
                ),
        )
        .child(label("Dialog and AlertDialog"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Button::new("rename-button", "Rename file")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| open_rename(window, cx)),
                )
                .child(
                    Button::new("delete-button", "Delete project")
                        .variant(ButtonVariant::Destructive)
                        .on_click(move |_, window, cx| open_delete(window, cx)),
                ),
        )
        .child(
            Dialog::new("rename", *rename_open.read(cx))
                .purpose(DialogPurpose::Form)
                .on_open_change(move |open, window, cx| {
                    if !open {
                        close_rename(window, cx)
                    }
                })
                .child(
                    crate::layout::Layout::new()
                        .header(
                            DialogHeader::new("Rename file")
                                .subtitle("Names show in the sidebar and in links.")
                                .on_open_change(move |_, window, cx| close_rename_header(window, cx)),
                        )
                        .content(
                            crate::layout::LayoutContent::new("rename-body")
                                .child(TextInput::new("file-name", "Name", "Quarterly report")),
                        )
                        .footer(
                            crate::layout::LayoutFooter::new().child(
                                Stack::horizontal()
                                    .gap(SpacingStep::S2)
                                    .h_align(crate::layout::StackAlign::End)
                                    .child(
                                        Button::new("rename-cancel", "Cancel")
                                            .variant(ButtonVariant::Ghost)
                                            .on_click(move |_, window, cx| close_rename_cancel(window, cx)),
                                    )
                                    .child(Button::new("rename-save", "Rename")),
                            ),
                        ),
                ),
        )
        .child(
            AlertDialog::new(
                "delete",
                *delete_open.read(cx),
                "Delete project?",
                "The project and its 12 files will be deleted. This cannot be undone.",
                "Delete project",
            )
            .on_action({
                let close = close_delete.clone();
                move |window, cx| close(window, cx)
            })
            .on_open_change(move |open, window, cx| {
                if !open {
                    close_delete(window, cx)
                }
            }),
        )
        .child(label("Drawer and BottomSheet"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Button::new("drawer-button", "Inspect order")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| open_drawer(window, cx)),
                )
                .child(
                    Button::new("sheet-button", "Share")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| open_sheet(window, cx)),
                )
                .child(
                    Popover::new("sheet-popover", "Sort by")
                        .presentation(PopoverPresentation::Adaptive)
                        .content(
                            Stack::vertical()
                                .child(Button::new("sort-new", "Newest").variant(ButtonVariant::Ghost))
                                .child(Button::new("sort-old", "Oldest").variant(ButtonVariant::Ghost)),
                        )
                        .child(Button::new("sort-button", "Sort").variant(ButtonVariant::Secondary)),
                ),
        )
        .child(
            Drawer::new("order-drawer", *drawer_open.read(cx), "Order 1042")
                .on_open_change(move |open, window, cx| {
                    if !open {
                        close_drawer(window, cx)
                    }
                })
                .child(
                    crate::layout::Layout::new()
                        .header(
                            DrawerHeader::new("Order 1042")
                                .subtitle("Placed 2 hours ago")
                                .on_open_change(move |_, window, cx| close_drawer_header(window, cx)),
                        )
                        .content(
                            crate::layout::LayoutContent::new("order-body").child(
                                Stack::vertical()
                                    .gap(SpacingStep::S3)
                                    .child(
                                        Text::new("3 items, shipping to Lisbon.")
                                            .text_type(TextType::Body),
                                    )
                                    .child(TextInput::new("order-note", "Note", "")),
                            ),
                        ),
                ),
        )
        .child({
            // Drag the grab bar, or pull the content at its top: the sheet rests at
            // 30% or fully open, and a flick down closes it. "More" stacks a second
            // sheet over it.
            let more_open = window.use_keyed_state("sheet-more-open", cx, |_, _| false);
            let (open_more, close_more) = (toggle(&more_open, true), toggle(&more_open, false));
            BottomSheet::new("share-sheet", *sheet_open.read(cx), "Share")
                .height(SheetHeight::Capped)
                .snap_points([SnapPoint::Fraction(0.3)])
                .on_open_change(move |open, window, cx| {
                    if !open {
                        close_sheet(window, cx)
                    }
                })
                .child(
                    Stack::vertical()
                        .gap(SpacingStep::S2)
                        .child(Text::new("Share").text_type(TextType::Label))
                        .child(Button::new("share-copy", "Copy link").variant(ButtonVariant::Ghost))
                        .child(Button::new("share-mail", "Email").variant(ButtonVariant::Ghost))
                        .child(
                            Button::new("share-more", "More options")
                                .variant(ButtonVariant::Secondary)
                                .on_click(move |_, window, cx| open_more(window, cx)),
                        )
                        .children((0..12).map(|i| {
                            Text::new(format!("Recent contact {}", i + 1)).text_type(TextType::Body)
                        })),
                )
                .child(
                    BottomSheet::new("share-more-sheet", *more_open.read(cx), "More options")
                        .height(SheetHeight::Hug)
                        .on_open_change(move |open, window, cx| {
                            if !open {
                                close_more(window, cx)
                            }
                        })
                        .child(
                            Stack::vertical()
                                .gap(SpacingStep::S2)
                                .child(Text::new("More options").text_type(TextType::Label))
                                .child(Button::new("share-print", "Print").variant(ButtonVariant::Ghost))
                                .child(Button::new("share-save", "Save to files").variant(ButtonVariant::Ghost)),
                        ),
                )
        })
        .child(label("DropdownMenu, MoreMenu and ContextMenu"))
        .child(menus(window, cx))
        .child(label("Lightbox"))
        .child(lightbox(window, cx))
        .child(label("CommandPalette"))
        .child(command_palette(window, cx))
        .child(label("Toast"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Button::new("toast-info", "Save")
                        .variant(ButtonVariant::Secondary)
                        .on_click(|_, window, cx| {
                            show_toast(
                                ToastOptions::new("Changes saved").unique_id("saved"),
                                window,
                                cx,
                            );
                        }),
                )
                .child(
                    Button::new("toast-undo", "Archive")
                        .variant(ButtonVariant::Secondary)
                        .on_click(|_, window, cx| {
                            show_toast(
                                ToastOptions::new("Conversation archived").end_content(|_, _| {
                                    Button::new("toast-undo-action", "Undo")
                                        .variant(ButtonVariant::Ghost)
                                        .into_any_element()
                                }),
                                window,
                                cx,
                            );
                        }),
                )
                .child(
                    Button::new("toast-error", "Send")
                        .variant(ButtonVariant::Secondary)
                        .on_click(|_, window, cx| {
                            show_toast(
                                ToastOptions::new("Message not sent. Check your connection.")
                                    .kind(ToastType::Error),
                                window,
                                cx,
                            );
                        }),
                ),
        )
        .child(label("Overlay"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Overlay::new("cover-hover")
                        .show_on(OverlayShowOn::Hover)
                        .position(OverlayPosition::Bottom)
                        .content(Text::new("Sunset over the bay").text_type(TextType::Label))
                        .child(cover_art(t_swatch(window, cx, 0))),
                )
                .child(
                    Overlay::new("cover-always")
                        .content(Text::new("12 photos").text_type(TextType::Label))
                        .child(cover_art(t_swatch(window, cx, 1))),
                ),
        )
        .child(label("HoverCard"))
        .child(
            Text::new("Reviewed by ")
                .text_type(TextType::Body)
                .child(
                    HoverCard::new("reviewer")
                        .label("Ada Lovelace")
                        .content(
                            Stack::vertical()
                                .gap(SpacingStep::S2)
                                .width(gpui::px(240.))
                                .child(Text::new("Ada Lovelace").text_type(TextType::Label))
                                .child(
                                    Text::new("Analyst. Wrote the first published program.")
                                        .text_type(TextType::Supporting),
                                )
                                .child(Button::new("reviewer-follow", "Follow")),
                        )
                        .text("Ada Lovelace"),
                )
                .child(" two days ago."),
        )
        .child(label("InfoTip"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(Text::new("Monthly active users").text_type(TextType::Body))
                .child(InfoTip::new(
                    "mau",
                    "People who signed in at least once in the last 30 days.",
                )),
        )
        .child(label("Tooltips in fields"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(gpui::px(360.))
                .child(
                    TextInput::new("workspace-name", "Workspace name", "Astryx")
                        .label_tooltip("Shown in the sidebar and in invitations."),
                )
                .child(
                    TextInput::new("billing", "Billing email", "")
                        .disabled(true)
                        .disabled_message("Only owners can change billing."),
                )
                .child(
                    TextInput::new("slug", "Slug", "astryx rust")
                        .status(InputStatus::error("Use letters, numbers and dashes."))
                        .status_placement(crate::field::StatusPlacement::Tooltip),
                ),
        )
        .into_any_element()
}

/// A swatch color for an example cover.
fn t_swatch(window: &mut Window, cx: &mut App, i: usize) -> gpui::Hsla {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    if i == 0 {
        t.color.accent
    } else {
        t.color.success
    }
}

/// A stand-in for a photo: a colored 200 x 120 card.
fn cover_art(color: gpui::Hsla) -> impl IntoElement {
    use gpui::Styled as _;
    gpui::div()
        .w(gpui::px(200.))
        .h(gpui::px(120.))
        .rounded(gpui::px(8.))
        .bg(color)
}

/// A toast for a menu pick.
fn picked(text: &'static str) -> impl Fn(&mut Window, &mut App) + 'static {
    move |window, cx| {
        show_toast(ToastOptions::new(text), window, cx);
    }
}

fn menus(window: &mut Window, cx: &mut App) -> AnyElement {
    let sort = window.use_keyed_state("menu-sort", cx, |_, _| SharedString::from("newest"));
    let wrap = window.use_keyed_state("menu-wrap", cx, |_, _| true);
    let (sort_value, wrap_value) = (sort.read(cx).clone(), *wrap.read(cx));
    let border = astryx_theme::current(window, cx).tokens().color.border;

    let view = DropdownMenu::new("menu-view")
        .button_label("View")
        .entry(
            MenuCheckboxItem::new("wrap", "Word wrap", wrap_value).on_change(
                move |on, window, cx| {
                    wrap.update(cx, |w, _| *w = on);
                    window.refresh();
                },
            ),
        )
        .entry(MenuEntry::Divider)
        .entry(
            MenuRadioGroup::new(sort_value)
                .title("Sort by")
                .on_change(move |value, window, cx| {
                    sort.update(cx, |s, _| *s = value);
                    window.refresh();
                })
                .item(MenuRadioItem::new("newest", "Newest").icon("clock"))
                .item(MenuRadioItem::new("oldest", "Oldest"))
                .item(MenuRadioItem::new("name", "Name")),
        );

    let actions = DropdownMenu::new("menu-actions")
        .button_label("Actions")
        .entry(
            MenuItem::new("duplicate", "Duplicate")
                .icon("copy")
                .end_content(|_, _| crate::content::Kbd::new("mod+d").into_any_element())
                .on_click(picked("Duplicated")),
        )
        .entry(
            MenuItem::new("move", "Move to").submenu([
                MenuItem::new("inbox", "Inbox")
                    .on_click(picked("Moved to Inbox"))
                    .into(),
                MenuItem::new("archive", "Archive")
                    .on_click(picked("Archived"))
                    .into(),
                MenuItem::new("later", "Later")
                    .submenu([
                        MenuItem::new("tomorrow", "Tomorrow")
                            .on_click(picked("Snoozed until tomorrow"))
                            .into(),
                        MenuItem::new("next-week", "Next week")
                            .on_click(picked("Snoozed until next week"))
                            .into(),
                    ])
                    .into(),
            ]),
        )
        .entry(
            MenuItem::new("labels", "Labels")
                .spinner(true)
                .submenu([MenuItem::new("loading", "Loading…").disabled(true).into()]),
        )
        .entry(
            MenuItem::new("share", "Share")
                .description("Anyone with the link can view")
                .disabled(true),
        )
        .entry(MenuEntry::Divider)
        .entry(
            MenuItem::new("delete", "Delete")
                .destructive(true)
                .on_click(picked("Deleted")),
        );

    let filters = DropdownMenu::new("menu-filters")
        .button_label("Filters")
        .header(Text::new("Show conversations").text_type(TextType::Supporting))
        .footer(Button::new("menu-filters-reset", "Reset filters").variant(ButtonVariant::Ghost))
        .items([
            MenuEntry::section(
                "Status",
                [
                    MenuItem::new("open", "Open").into(),
                    MenuItem::new("closed", "Closed").into(),
                ],
            ),
            MenuEntry::section(
                "Owner",
                [
                    MenuItem::new("mine", "Assigned to me").into(),
                    MenuItem::new("anyone", "Anyone").into(),
                ],
            ),
        ]);

    let more = MoreMenu::new(
        "menu-more",
        [
            MenuItem::new("rename", "Rename")
                .on_click(picked("Renamed"))
                .into(),
            MenuItem::new("pin", "Pin")
                .on_click(picked("Pinned"))
                .into(),
            MenuEntry::Divider,
            MenuItem::new("leave", "Leave")
                .destructive(true)
                .on_click(picked("Left the conversation"))
                .into(),
        ],
    );

    let area = ContextMenu::new("menu-context")
        .items([
            MenuItem::new("open", "Open")
                .on_click(picked("Opened"))
                .into(),
            MenuItem::new("copy-link", "Copy link")
                .icon("copy")
                .on_click(picked("Link copied"))
                .into(),
            MenuEntry::Divider,
            MenuItem::new("remove", "Remove")
                .destructive(true)
                .on_click(picked("Removed"))
                .into(),
        ])
        .child(
            div()
                .w(px(320.))
                .h(px(96.))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_dashed()
                .border_color(border)
                .rounded(px(8.))
                .child(
                    Text::new("Right-click, or Shift+F10, here").text_type(TextType::Supporting),
                ),
        );

    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(view)
                .child(actions)
                .child(filters)
                .child(more),
        )
        .child(area)
        .child(
            TextInput::new(
                "menu-spelling",
                "Right-click a misspelled word",
                "Teh quick brwon fox",
            )
            .width(px(320.)),
        )
        .into_any_element()
}

/// A photo drawn in memory: a two-colour gradient, `w` × `h` pixels.
fn gradient(w: u32, h: u32, from: [u8; 3], to: [u8; 3]) -> std::sync::Arc<gpui::RenderImage> {
    let buffer = image::RgbaImage::from_fn(w, h, |x, y| {
        let t = (x as f32 / w as f32 + y as f32 / h as f32) / 2.;
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        // BGRA, as GPUI's render images are.
        image::Rgba([
            mix(from[2], to[2]),
            mix(from[1], to[1]),
            mix(from[0], to[0]),
            255,
        ])
    });
    std::sync::Arc::new(gpui::RenderImage::new(smallvec::smallvec![
        image::Frame::new(buffer)
    ]))
}

type Photo = (std::sync::Arc<gpui::RenderImage>, &'static str);

fn lightbox(window: &mut Window, cx: &mut App) -> AnyElement {
    let photos = window.use_keyed_state("lightbox-photos", cx, |_, _| -> Vec<Photo> {
        vec![
            (
                gradient(1600, 1000, [36, 77, 140], [214, 120, 64]),
                "Dusk over the bay",
            ),
            (gradient(900, 1200, [20, 110, 80], [230, 220, 150]), "Fern"),
            (
                gradient(2000, 1100, [120, 40, 90], [250, 190, 120]),
                "Dunes at noon",
            ),
        ]
    });
    let photos = photos.read(cx).clone();
    let media = photos.iter().enumerate().map(|(i, (photo, alt))| {
        let item = LightboxMedia::from_source(photo.clone(), *alt);
        match i {
            0 => item.caption("Taken from the ridge an hour after sunset."),
            2 => item.caption(
                "The dunes shift a few metres every year. This one, photographed at noon in \
                 late summer, had moved far enough to cover the old trail markers on its \
                 eastern face, which the park later replaced with taller posts.",
            ),
            _ => item,
        }
    });
    let lightbox = use_lightbox("lightbox", media, window, cx);
    let thumbs: Vec<AnyElement> = photos
        .iter()
        .enumerate()
        .map(|(i, (photo, _))| {
            lightbox
                .trigger(
                    i,
                    div()
                        .w(px(96.))
                        .h(px(72.))
                        .rounded(px(8.))
                        .overflow_hidden()
                        .child(
                            gpui::img(gpui::ImageSource::Render(photo.clone()))
                                .size_full()
                                .object_fit(gpui::ObjectFit::Cover),
                        ),
                )
                .into_any_element()
        })
        .collect();
    Stack::horizontal()
        .gap(SpacingStep::S2)
        .children(thumbs)
        .child(lightbox.element(cx).has_zoom(true))
        .into_any_element()
}

fn command_palette(window: &mut Window, cx: &mut App) -> AnyElement {
    let open = window.use_keyed_state("palette-open", cx, |_, _| false);
    let shown = *open.read(cx);
    let (show, report) = (open.clone(), open.clone());
    let commands = [
        SearchItem::new("new-file", "New file").group("File"),
        SearchItem::new("open-file", "Open file…").group("File"),
        SearchItem::new("save", "Save").group("File"),
        SearchItem::new("theme", "Toggle theme")
            .group("View")
            .keywords(["dark", "light"]),
        SearchItem::new("zoom-in", "Zoom in").group("View"),
        SearchItem::new("shortcuts", "Keyboard shortcuts"),
    ];
    Stack::horizontal()
        .gap(SpacingStep::S2)
        .child(
            Button::new("palette-open", "Open the command palette")
                .variant(ButtonVariant::Secondary)
                .on_click(move |_, window, cx| {
                    show.update(cx, |v, _| *v = true);
                    window.refresh();
                }),
        )
        .child(
            CommandPalette::new(
                "palette",
                shown,
                static_source(commands).with_item_keywords(),
            )
            .on_open_change(move |value, window, cx| {
                report.update(cx, |v, _| *v = value);
                window.refresh();
            })
            .on_value_change(|id, window, cx| {
                show_toast(ToastOptions::new(format!("Ran {id}")), window, cx);
            }),
        )
        .into_any_element()
}
