//! FileInput. Upstream: `FileInput/`.
//!
//! The platform's file dialog stands in for `<input type="file">`, and in
//! dropzone mode files dragged from the system drop onto the field. A file
//! is a [`PickedFile`]: its path, name, size and MIME type, and
//! [`PickedFile::read`] reads it. In the browser the fork's web platform
//! (patch R8) opens the browser's file dialog, takes dropped files, and
//! gives each a virtual path.

use std::path::PathBuf;
use std::rc::Rc;

use astryx_i18n::{I18n, current as i18n};
use astryx_primitives::{
    ElementSize, Politeness, announce, control_focus, is_focus_visible, provide,
};
use astryx_theme::color::mix_srgb;
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    App, ElementId, ExternalPaths, Hsla, IntoElement, Length, ParentElement, PathPromptOptions,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};

use super::control::{described_by, status_indicator};
use crate::button::{ButtonVariant, IconButton};
use crate::choice::action::{ActionFn, ChangeFn, Optimistic};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType, resolve};
use crate::icon::{Icon, IconColor, IconSize};
use crate::status::{Spinner, SpinnerSize};

/// A chosen file, upstream `File`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedFile {
    pub path: PathBuf,
    pub name: SharedString,
    /// Bytes.
    pub size: u64,
    /// The MIME type its extension names (`image/png`), or empty.
    pub mime: SharedString,
}

impl PickedFile {
    /// A file on disk, or a browser file's virtual path: its name, size and
    /// type.
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        #[cfg(target_family = "wasm")]
        if let Some(file) = gpui_web::web_files::info(&path) {
            let mime = if file.mime.is_empty() {
                path.extension()
                    .and_then(|e| mime_for_extension(&e.to_string_lossy()))
                    .unwrap_or_default()
                    .to_string()
            } else {
                file.mime
            };
            return PickedFile {
                path,
                name: file.name.into(),
                size: file.size,
                mime: mime.into(),
            };
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
        let mime = path
            .extension()
            .and_then(|e| mime_for_extension(&e.to_string_lossy()))
            .unwrap_or_default();
        PickedFile {
            path,
            name: name.into(),
            size,
            mime: mime.into(),
        }
    }
}

impl PickedFile {
    /// Reads the file's bytes: from disk on the background executor, or in
    /// the browser from the file it handed over.
    pub fn read(&self, cx: &App) -> Task<anyhow::Result<Vec<u8>>> {
        let path = self.path.clone();
        #[cfg(target_family = "wasm")]
        {
            cx.foreground_executor()
                .spawn(async move { gpui_web::web_files::read(&path).await })
        }
        #[cfg(not(target_family = "wasm"))]
        {
            cx.background_executor()
                .spawn(async move { Ok(std::fs::read(&path)?) })
        }
    }
}

/// The MIME type of common file extensions, as browsers report `File.type`.
pub fn mime_for_extension(extension: &str) -> Option<&'static str> {
    Some(match extension.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "xml" => "application/xml",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "md" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return None,
    })
}

/// Upstream `mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileInputMode {
    /// A compact field.
    #[default]
    Input,
    /// A tall surface files can be dropped on.
    Dropzone,
}

impl FileInputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            FileInputMode::Input => "input",
            FileInputMode::Dropzone => "dropzone",
        }
    }
}

/// Upstream `formatFileSize`.
fn file_size(bytes: u64, i18n: &I18n) -> SharedString {
    let b = bytes as f64;
    if bytes < 1024 {
        i18n.format("@astryx.fileInput.fileSize.bytes", &[("size", b.into())])
    } else if bytes < 1024 * 1024 {
        i18n.format(
            "@astryx.fileInput.fileSize.kilobytes",
            &[("size", (b / 1024.).into())],
        )
    } else {
        i18n.format(
            "@astryx.fileInput.fileSize.megabytes",
            &[("size", (b / (1024. * 1024.)).into())],
        )
    }
}

/// Upstream `validateFiles`: the accepted files and why the rest were not.
pub(crate) fn validate(
    files: Vec<PickedFile>,
    accept: Option<&str>,
    max_size: Option<u64>,
    max_files: Option<usize>,
    multiple: bool,
    i18n: &I18n,
) -> (Vec<PickedFile>, Vec<SharedString>) {
    let mut errors = Vec::new();
    let mut valid = files;
    if let Some(accept) = accept.filter(|a| !a.is_empty()) {
        let types: Vec<String> = accept.split(',').map(|s| s.trim().to_lowercase()).collect();
        valid.retain(|file| {
            let name = file.name.to_lowercase();
            let mime = file.mime.to_lowercase();
            let matches = types.iter().any(|ty| {
                if ty.starts_with('.') {
                    name.ends_with(ty.as_str())
                } else if let Some(family) = ty.strip_suffix('*') {
                    mime.starts_with(family)
                } else {
                    mime == *ty
                }
            });
            if !matches {
                errors.push(i18n.format(
                    "@astryx.fileInput.errorInvalidType",
                    &[("fileName", file.name.as_ref().into())],
                ));
            }
            matches
        });
    }
    if let Some(max) = max_size {
        valid.retain(|file| {
            if file.size > max {
                let limit = file_size(max, i18n);
                errors.push(i18n.format(
                    "@astryx.fileInput.errorMaxSize",
                    &[
                        ("fileName", file.name.as_ref().into()),
                        ("maxSize", limit.as_ref().into()),
                    ],
                ));
                false
            } else {
                true
            }
        });
    }
    if multiple
        && let Some(max) = max_files
        && valid.len() > max
    {
        errors.push(i18n.format(
            "@astryx.fileInput.errorMaxFiles",
            &[("maxFiles", (max as f64).into())],
        ));
        valid.truncate(max);
    }
    (valid, errors)
}

#[derive(Default)]
struct FileState {
    /// The rejection of the last pick, shown as the error status.
    validation_error: Option<SharedString>,
    /// Picking announced its result, so the busy start is not.
    pick_announced: bool,
}

type FilesFn = Rc<dyn Fn(Vec<PickedFile>, &mut Window, &mut App)>;

/// A file picker with an input or dropzone look, upstream `FileInput`.
/// The value is the chosen files: empty for none, at most one unless
/// `multiple`.
#[derive(IntoElement)]
pub struct FileInput {
    id: ElementId,
    label: SharedString,
    value: Vec<PickedFile>,
    on_change: Option<ChangeFn<Vec<PickedFile>>>,
    change_action: Option<ActionFn<Vec<PickedFile>>>,
    accept: Option<SharedString>,
    multiple: bool,
    max_size: Option<u64>,
    max_files: Option<usize>,
    label_hidden: bool,
    description: Option<SharedString>,
    optional: bool,
    required: bool,
    disabled: bool,
    disabled_message: Option<SharedString>,
    loading: bool,
    placeholder: Option<SharedString>,
    mode: FileInputMode,
    status: Option<InputStatus>,
    status_placement: Option<StatusPlacement>,
    label_tooltip: Option<SharedString>,
    width: Option<Length>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl FileInput {
    /// Upstream `label` and the controlled `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<Vec<PickedFile>>,
    ) -> Self {
        FileInput {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: None,
            change_action: None,
            accept: None,
            multiple: false,
            max_size: None,
            max_files: None,
            label_hidden: false,
            description: None,
            optional: false,
            required: false,
            disabled: false,
            disabled_message: None,
            loading: false,
            placeholder: None,
            mode: FileInputMode::Input,
            status: None,
            status_placement: None,
            label_tooltip: None,
            width: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `accept`: file types as the HTML attribute lists them,
        /// such as `image/*,.pdf`. Other files are refused with a message.
        ?accept: SharedString,
        /// Upstream `isMultiple`.
        !multiple,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `disabledMessage`: why it is disabled. It describes the
        /// field now; its tooltip lands with Tooltip (M5).
        ?disabled_message: SharedString,
        /// Upstream `isLoading`.
        !loading,
        /// Upstream `placeholder` (default "Choose file" or "Choose files").
        ?placeholder: SharedString,
        /// Upstream `mode`.
        mode: FileInputMode,
        /// Upstream `status`. A rejected pick shows its own error status.
        ?status: InputStatus,
        /// Upstream `statusVariant`: attached in input mode, detached in a
        /// dropzone (`tooltip` lands with Tooltip, M5).
        ?status_placement: StatusPlacement,
        /// Upstream `labelTooltip`. The InfoTip lands with Tooltip (M5).
        ?label_tooltip: SharedString,
        /// Upstream `width`.
        ?width: Length,
    }

    /// Upstream `maxSize`, in bytes: larger files are refused.
    pub fn max_size(mut self, bytes: u64) -> Self {
        self.max_size = Some(bytes);
        self
    }

    /// Upstream `maxFiles` (with `multiple`): files past it are dropped.
    pub fn max_files(mut self, count: usize) -> Self {
        self.max_files = Some(count);
        self
    }

    /// Upstream `onChange`: the chosen files, or none when cleared or when
    /// every file was refused.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<PickedFile>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`, such as an
    /// upload; the field shows the new files and is busy until it settles.
    pub fn change_action(
        mut self,
        f: impl Fn(Vec<PickedFile>, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`: styles the surface.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for FileInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What picking and dropping share.
#[derive(Clone)]
struct Picker {
    state: gpui::Entity<FileState>,
    change: FilesFn,
    accept: Option<SharedString>,
    max_size: Option<u64>,
    max_files: Option<usize>,
    multiple: bool,
    inert: bool,
}

impl Picker {
    /// Upstream `handleFiles`.
    fn files(&self, files: Vec<PickedFile>, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        let i18n = i18n(cx);
        let (valid, errors) = validate(
            files,
            self.accept.as_deref(),
            self.max_size,
            self.max_files,
            self.multiple,
            &i18n,
        );
        let error = (!errors.is_empty())
            .then(|| SharedString::from(i18n.conjunction(errors.iter().map(|e| e.as_ref()))));
        self.state.update(cx, |s, _| s.validation_error = error);
        if valid.is_empty() {
            (self.change)(Vec::new(), window, cx);
            window.refresh();
            return;
        }
        let result: Vec<PickedFile> = if self.multiple {
            valid
        } else {
            valid.into_iter().take(1).collect()
        };
        let message = match result.as_slice() {
            [one] => i18n.format(
                "@astryx.fileInput.fileSelected",
                &[("fileName", one.name.as_ref().into())],
            ),
            many => i18n.format(
                "@astryx.fileInput.filesSelected",
                &[("count", (many.len() as f64).into())],
            ),
        };
        if errors.is_empty() {
            self.state.update(cx, |s, _| s.pick_announced = true);
            announce(message, Politeness::Polite, window, cx);
        }
        (self.change)(result, window, cx);
        window.refresh();
    }

    /// Opens the platform's file dialog.
    fn open(&self, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: self.multiple,
            prompt: None,
        });
        let picker = self.clone();
        window
            .spawn(cx, async move |cx| {
                let Ok(Ok(Some(paths))) = paths.await else {
                    return;
                };
                let files: Vec<PickedFile> = paths.into_iter().map(PickedFile::from_path).collect();
                cx.update(|window, cx| picker.files(files, window, cx)).ok();
            })
            .detach();
    }
}

impl RenderOnce for FileInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let i18n = i18n(cx);
        let rem = window.rem_size();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-files", self.id).into()),
            cx,
            |_, _| FileState::default(),
        );

        let optimistic = Optimistic::<Vec<PickedFile>>::new(&self.id, window, cx);
        let shown = optimistic.shown(self.value.clone(), cx);
        let busy = self.loading || optimistic.pending(cx);
        let pick_announced = state.read(cx).pick_announced;
        optimistic.announce_busy(
            busy && !pick_announced,
            i18n.t("@astryx.spinner.loading"),
            window,
            cx,
        );
        if !busy && pick_announced {
            state.update(cx, |s, _| s.pick_announced = false);
        }
        let change: FilesFn = Rc::new(optimistic.change(self.on_change, self.change_action));
        let inert = self.disabled || busy;
        let picker = Picker {
            state: state.clone(),
            change: change.clone(),
            accept: self.accept.clone(),
            max_size: self.max_size,
            max_files: self.max_files,
            multiple: self.multiple,
            inert,
        };

        let status = self.status.clone().or_else(|| {
            state
                .read(cx)
                .validation_error
                .clone()
                .map(InputStatus::error)
        });
        let dropzone = self.mode == FileInputMode::Dropzone;
        let placement = self.status_placement.unwrap_or(if dropzone {
            StatusPlacement::Detached
        } else {
            StatusPlacement::Attached
        });
        let status_type = status.as_ref().map(|s| s.status);
        let mut props = vec![("mode", self.mode.as_str())];
        if let Some(status) = status_type {
            props.push(("status", status.as_str()));
        }
        let part_override = theme.for_part("file-input", &props);
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();

        let focus = control_focus(&self.id, window, cx);
        let focused = focus.is_focused(window);
        let has_files = !shown.is_empty();
        let names = shown
            .iter()
            .map(|f| f.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");
        let placeholder = self.placeholder.clone().unwrap_or_else(|| {
            i18n.t(if self.multiple {
                "@astryx.fileInput.placeholderMultiple"
            } else {
                "@astryx.fileInput.placeholder"
            })
        });

        // The surface: its focus, status and disabled looks.
        let rest_border = match status_type {
            Some(StatusType::Error) => t.color.border_error,
            Some(StatusType::Warning) => t.color.border_warning,
            Some(StatusType::Success) => t.color.border_success,
            None => t.color.border_emphasized,
        };
        let ring = if self.disabled {
            is_focus_visible(&focus, window)
        } else {
            focused
        };
        let border = if ring && status_type.is_none() {
            t.color.accent
        } else {
            rest_border
        };
        let hover_ring = mix_srgb(Hsla::transparent_black(), t.color.accent, 0.2);
        let group: SharedString = format!("{}-surface", self.id).into();
        let mut surface = div()
            .id(ElementId::Name(format!("{}-control", self.id).into()))
            .group(group.clone())
            .relative()
            .flex()
            .border(t.border.width)
            .border_color(border)
            .rounded(t.radius.element)
            .bg(t.color.background_surface)
            .track_focus(&focus)
            .role(Role::Button)
            .aria_label(if has_files {
                i18n.format(
                    "@astryx.fileInput.triggerWithFiles",
                    &[
                        ("label", self.label.as_ref().into()),
                        ("fileNames", names.as_str().into()),
                    ],
                )
            } else {
                self.label.clone()
            })
            .aria_disabled(inert)
            .aria_busy(busy)
            .aria_invalid(status_type == Some(StatusType::Error));
        let mut described = described_by(
            self.description.as_ref(),
            status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        if resolve(self.required, self.optional, cx).required {
            described.push(i18n.t("@astryx.fileInput.required"));
        }
        if !described.is_empty() {
            surface = surface.aria_description(SharedString::from(described.join(" ")));
        }
        surface = if dropzone {
            surface
                .flex_col()
                .items_center()
                .justify_center()
                .gap(t.spacing.s2)
                .py(t.spacing.s6)
                .px(t.spacing.s4)
                .border_dashed()
        } else {
            let line = t.text.body.size.to_pixels(rem) * t.text.body.leading;
            let height = t.size.element_md.to_pixels(rem);
            let pad = ((height - line - t.border.width.to_pixels(rem) * 2.) / 2.).max(px(0.));
            surface
                .flex_row()
                .items_center()
                .gap(t.spacing.s2)
                .py(pad)
                .px(t.spacing.s2)
                .min_h(height)
        };
        if self.disabled {
            surface = surface.opacity(0.5);
        }
        if !inert {
            surface = surface.cursor_pointer();
            if !(focused && !dropzone) {
                let inset = gpui::BoxShadow {
                    color: hover_ring,
                    offset: gpui::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(2.),
                    inset: true,
                };
                surface = surface.hover(move |s| {
                    let mut s = s;
                    s.box_shadow = Some(vec![inset.clone()]);
                    s
                });
            }
            // Enter and Space reach on_click as a keyboard click.
            let click = picker.clone();
            surface = surface.on_click(move |_, window, cx| click.open(window, cx));
            if dropzone {
                let (accent, muted) = (t.color.accent, t.color.accent_muted);
                let drop = picker.clone();
                surface = surface
                    .drag_over::<ExternalPaths>(move |s, _, _, _| s.border_color(accent).bg(muted))
                    .on_drop(move |paths: &ExternalPaths, window, cx| {
                        let files = paths
                            .paths()
                            .iter()
                            .cloned()
                            .map(PickedFile::from_path)
                            .collect();
                        drop.files(files, window, cx);
                    });
            }
        }
        part.refine(surface.style(), "file-input", &props, None);
        surface.style().refine(&self.control_style);

        // The content: the upload glyph, the names or placeholder, a spinner
        // while busy, and a clear button.
        let text = |content: SharedString, filled: bool, center: bool| {
            let mut el = div()
                .font_stack(&t.font.family.body)
                .text_style(&t.text.body)
                .text_color(if filled {
                    t.color.text_primary
                } else {
                    t.color.text_secondary
                });
            el = if center {
                el.text_center()
            } else {
                el.flex_1().min_w_0()
            };
            if filled && !center {
                el = el.truncate();
            }
            el.child(content)
        };
        let glyph = |size: IconSize| {
            let mut icon = Icon::new("upload").size(size).color(IconColor::Secondary);
            part.refine(
                icon.style(),
                "file-input-icon",
                &[("mode", self.mode.as_str())],
                None,
            );
            icon
        };
        if dropzone {
            if busy {
                surface = surface.child(Spinner::new("spinner").size(SpinnerSize::Md).decorative());
            } else if has_files {
                surface = surface.child(text(names.clone().into(), true, true));
            } else {
                let hint = i18n.t("@astryx.fileInput.dropHint");
                let group_hide = group.clone();
                surface = surface
                    .child(glyph(IconSize::Md))
                    .child(
                        text(placeholder.clone(), false, true)
                            .group_drag_over::<ExternalPaths>(group_hide, |s| {
                                s.invisible().absolute()
                            }),
                    )
                    .child(
                        text(hint, false, true)
                            .invisible()
                            .absolute()
                            .group_drag_over::<ExternalPaths>(group.clone(), |s| {
                                s.visible().relative()
                            }),
                    );
            }
        } else if busy {
            surface = surface
                .child(text(
                    if has_files {
                        names.clone().into()
                    } else {
                        placeholder.clone()
                    },
                    has_files,
                    false,
                ))
                .child(Spinner::new("spinner").size(SpinnerSize::Sm).decorative());
        } else {
            surface = surface.child(glyph(IconSize::Sm)).child(text(
                if has_files {
                    names.clone().into()
                } else {
                    placeholder.clone()
                },
                has_files,
                false,
            ));
            if let Some(kind) = status_type
                && placement != StatusPlacement::Detached
            {
                let message = status.as_ref().and_then(|s| s.message.clone());
                surface = surface.child(status_indicator(
                    &self.id,
                    kind,
                    placement,
                    message.as_ref(),
                    &part,
                    window,
                    cx,
                ));
            }
        }
        if has_files && !self.disabled && !busy {
            let label = i18n.format(
                "@astryx.fileInput.clearLabel",
                &[("label", self.label.as_ref().into())],
            );
            let (state, change, focus) = (state.clone(), change.clone(), focus.clone());
            let clear = IconButton::new(
                "clear",
                label,
                Icon::new("close")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .tooltip(i18n.t("@astryx.inputClearButton.tooltip"))
            .h(px(20.))
            .on_click(move |_, window, cx| {
                state.update(cx, |s, _| s.validation_error = None);
                change(Vec::new(), window, cx);
                window.focus(&focus, cx);
                window.refresh();
            });
            surface = surface.child(clear);
        }

        let surface = match part_override {
            Some(part) => scope(part, surface).into_any_element(),
            None => surface.into_any_element(),
        };
        let surface = crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            false,
            surface,
        );
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(placement);
        if let Some(status) = status {
            field = field.status(status);
        }
        if let Some(tooltip) = self.label_tooltip {
            field = field.label_tooltip(tooltip);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field.style().refine(&self.style);
        field.child(provide(super::group::GroupSlot(None), surface))
    }
}
