//! MobileNavToggle, upstream `MobileNav/MobileNavToggle.tsx`: the menu button that opens
//! AppShell's mobile drawer. It renders only in AppShell's mobile layout with mobile
//! navigation on (FR11).

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::use_context;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, canvas, div, prelude::*,
};

use crate::app_shell::AppShellMobile;
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor};

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Upstream `MobileNavToggle`.
#[derive(IntoElement)]
pub struct MobileNavToggle {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<AnyElement>,
    on_click: Option<ClickFn>,
}

impl MobileNavToggle {
    pub fn new(id: impl Into<ElementId>) -> Self {
        MobileNavToggle {
            id: id.into(),
            label: None,
            icon: None,
            on_click: None,
        }
    }

    /// Upstream `label`: default "Open navigation".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `children`: replaces the menu icon.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `onClick`: after the toggle.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for MobileNavToggle {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(shell) = use_context::<AppShellMobile>(cx).filter(|s| s.is_mobile && s.enabled)
        else {
            return div().into_any_element();
        };
        let label = self
            .label
            .unwrap_or_else(|| i18n(cx).t("@astryx.mobileNav.toggle.open"));
        let icon = self.icon.unwrap_or_else(|| {
            Icon::new("menu")
                .color(IconColor::Inherit)
                .into_any_element()
        });
        let opener = shell.opener.clone();
        let (toggle, on_click) = (shell.clone(), self.on_click);
        let selector = format!("{}", self.id);
        div()
            .relative()
            .debug_selector(move || selector)
            .child(
                IconButton::new(self.id, label, icon)
                    .variant(ButtonVariant::Ghost)
                    .expanded(shell.is_open)
                    .on_click(move |event, window, cx| {
                        toggle.toggle(window, cx);
                        if let Some(f) = &on_click {
                            f(event, window, cx);
                        }
                    }),
            )
            // Where the toggle is, for the drawer's auto side.
            .child(
                canvas(
                    move |bounds, _, _| opener.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}
