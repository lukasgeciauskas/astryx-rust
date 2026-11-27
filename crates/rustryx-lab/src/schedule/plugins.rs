//! The built-in header plugins (upstream `plugins/`): the previous, Today and next controls,
//! and the view selector menu.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, IntoElement, SharedString, Window, div, px};
use rustryx_components::button::{Button, ButtonGroup, IconButton};
use rustryx_components::icon::{Icon, IconColor, IconSize};
use rustryx_components::overlay::{DropdownMenu, MenuEntry, MenuItem};
use rustryx_i18n::LogicalStyled as _;
use rustryx_primitives::ElementSize;

use super::context::use_schedule_context;
use super::types::{ScheduleHeaderContent, SchedulePlugin, SchedulePluginPosition, ScheduleView};

/// The pagination plugin's options (upstream `SchedulePaginationPluginOptions`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SchedulePaginationPluginOptions {
    /// Upstream `position`: the slot the controls go in (start).
    pub position: SchedulePluginPosition,
}

/// The previous, Today and next controls (upstream `SchedulePaginationControls`).
#[derive(IntoElement)]
struct SchedulePaginationControls;

impl RenderOnce for SchedulePaginationControls {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(context) = use_schedule_context(cx) else {
            return div().into_any_element();
        };
        // The chevrons mirror in a right-to-left layout (upstream `rtlStyles.mirror`).
        let dir = rustryx_i18n::use_direction(cx);
        let id = context.id.clone();
        let (previous, today, next) = (
            context.on_previous_date.clone(),
            context.on_today.clone(),
            context.on_next_date.clone(),
        );
        ButtonGroup::new(
            ElementId::Name(format!("{id}-pagination").into()),
            "Schedule pagination",
        )
        .size(ElementSize::Sm)
        .child(
            IconButton::new(
                ElementId::Name(format!("{id}-previous").into()),
                context.previous_date_label.clone(),
                Icon::new("chevronLeft")
                    .size(IconSize::Sm)
                    .color(IconColor::Inherit)
                    .mirror_in_rtl(dir),
            )
            .on_click(move |_, window, cx| previous(window, cx)),
        )
        .child(
            Button::new(ElementId::Name(format!("{id}-today").into()), "Today")
                .size(ElementSize::Sm)
                .on_click(move |_, window, cx| today(window, cx)),
        )
        .child(
            IconButton::new(
                ElementId::Name(format!("{id}-next").into()),
                context.next_date_label.clone(),
                Icon::new("chevronRight")
                    .size(IconSize::Sm)
                    .color(IconColor::Inherit)
                    .mirror_in_rtl(dir),
            )
            .on_click(move |_, window, cx| next(window, cx)),
        )
        .into_any_element()
    }
}

struct PaginationPlugin(SchedulePaginationPluginOptions);

impl SchedulePlugin for PaginationPlugin {
    fn render_header(&self, mut content: ScheduleHeaderContent) -> ScheduleHeaderContent {
        let controls = SchedulePaginationControls.into_any_element();
        match self.0.position {
            SchedulePluginPosition::Start => content.start_content.insert(0, controls),
            SchedulePluginPosition::End => content.end_content.push(controls),
        }
        content
    }
}

/// The previous, Today and next controls as a plugin (upstream
/// `useSchedulePaginationPlugin`).
pub fn schedule_pagination_plugin(
    options: SchedulePaginationPluginOptions,
) -> Rc<dyn SchedulePlugin> {
    Rc::new(PaginationPlugin(options))
}

/// The default plugins: the pagination controls (upstream `defaultSchedulePlugins`).
pub fn default_schedule_plugins() -> Vec<Rc<dyn SchedulePlugin>> {
    vec![schedule_pagination_plugin(
        SchedulePaginationPluginOptions::default(),
    )]
}

/// One view the selector offers (upstream `ScheduleViewSelectorOption`).
#[derive(Clone)]
pub struct ScheduleViewSelectorOption {
    /// Upstream `view`: the view the option switches to.
    pub view: ScheduleView,
    /// Upstream `label`: its name in the menu.
    pub label: SharedString,
}

impl ScheduleViewSelectorOption {
    /// A view and its menu label.
    pub fn new(view: ScheduleView, label: impl Into<SharedString>) -> Self {
        ScheduleViewSelectorOption {
            view,
            label: label.into(),
        }
    }
}

type ChangeView = Rc<dyn Fn(ScheduleView, &mut Window, &mut App)>;

/// The view selector plugin's options (upstream `ScheduleViewSelectorPluginOptions`).
#[derive(Clone)]
pub struct ScheduleViewSelectorPluginOptions {
    /// Upstream `onChangeView`: without it the menu's button is disabled.
    pub on_change_view: Option<ChangeView>,
    /// Upstream `position`: the slot the menu goes in (end).
    pub position: SchedulePluginPosition,
}

impl Default for ScheduleViewSelectorPluginOptions {
    fn default() -> Self {
        ScheduleViewSelectorPluginOptions {
            on_change_view: None,
            position: SchedulePluginPosition::End,
        }
    }
}

/// The view selector menu (upstream `ScheduleViewSelectorControl`).
#[derive(IntoElement)]
struct ScheduleViewSelectorControl {
    options: Rc<Vec<ScheduleViewSelectorOption>>,
    on_change_view: Option<ChangeView>,
}

impl RenderOnce for ScheduleViewSelectorControl {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(context) = use_schedule_context(cx) else {
            return div().into_any_element();
        };
        let selected = self
            .options
            .iter()
            .position(|o| o.view.is_same(&context.view));
        let label = selected.map_or(SharedString::from("View"), |i| {
            self.options[i].label.clone()
        });
        let disabled = self.on_change_view.is_none();
        let entries: Vec<MenuEntry> =
            self.options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let (change, view) = (self.on_change_view.clone(), option.view.clone());
                    let mut item = MenuItem::new(option.label.clone(), option.label.clone())
                        .on_click(move |window, cx| {
                            if let Some(change) = &change {
                                change(view.clone(), window, cx);
                            }
                        });
                    if Some(index) == selected {
                        item = item.end_content(|_, _| {
                            Icon::new("check")
                                .size(IconSize::Sm)
                                .color(IconColor::Primary)
                                .into_any_element()
                        });
                    }
                    item.into()
                })
                .collect();
        DropdownMenu::new(ElementId::Name(
            format!("{}-view-selector", context.id).into(),
        ))
        .button_label(label)
        .button_size(ElementSize::Sm)
        .button(move |b| b.disabled(disabled))
        .menu_width(px(160.))
        .items(entries)
        .into_any_element()
    }
}

struct ViewSelectorPlugin {
    options: Rc<Vec<ScheduleViewSelectorOption>>,
    plugin: ScheduleViewSelectorPluginOptions,
}

impl SchedulePlugin for ViewSelectorPlugin {
    fn render_header(&self, mut content: ScheduleHeaderContent) -> ScheduleHeaderContent {
        let control: AnyElement = ScheduleViewSelectorControl {
            options: self.options.clone(),
            on_change_view: self.plugin.on_change_view.clone(),
        }
        .into_any_element();
        match self.plugin.position {
            SchedulePluginPosition::Start => content.start_content.push(control),
            SchedulePluginPosition::End => content.end_content.push(control),
        }
        content
    }
}

/// A menu that switches between the given views (upstream
/// `useScheduleViewSelectorPlugin`).
pub fn schedule_view_selector_plugin(
    options: Vec<ScheduleViewSelectorOption>,
    plugin: ScheduleViewSelectorPluginOptions,
) -> Rc<dyn SchedulePlugin> {
    Rc::new(ViewSelectorPlugin {
        options: Rc::new(options),
        plugin,
    })
}
