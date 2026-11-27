//! FileInput tests.

use std::path::PathBuf;

use gpui::{ExternalPaths, FileDropEvent};
use rustryx_primitives::{Politeness, announce::announcement, announcer};

use super::*;
use crate::input::file_input::validate;

fn file(name: &str, size: u64) -> PickedFile {
    let mut file = PickedFile::from_path(PathBuf::from(name));
    file.size = size;
    file
}

#[test]
fn files_are_refused_by_type_size_and_count() {
    let en = rustryx_i18n::I18n::new("en");
    let (valid, errors) = validate(
        vec![file("a.png", 10), file("b.txt", 10), file("c.PDF", 10)],
        Some("image/*, .pdf"),
        None,
        None,
        true,
        &en,
    );
    assert_eq!(
        valid.iter().map(|f| f.name.as_ref()).collect::<Vec<_>>(),
        ["a.png", "c.PDF"]
    );
    assert_eq!(errors, ["\"b.txt\" is not an accepted file type"]);

    let (valid, errors) = validate(
        vec![file("a.png", 3 * 1024 * 1024)],
        None,
        Some(2 * 1024 * 1024),
        None,
        false,
        &en,
    );
    assert!(valid.is_empty());
    assert_eq!(errors, ["\"a.png\" exceeds 2.0 MB limit"]);

    let (valid, errors) = validate(
        vec![file("a.png", 1), file("b.png", 1), file("c.png", 1)],
        None,
        None,
        Some(2),
        true,
        &en,
    );
    assert_eq!(valid.len(), 2);
    assert_eq!(errors, ["Maximum 2 files allowed"]);
}

#[test]
fn types_come_from_extensions() {
    assert_eq!(mime_for_extension("PNG"), Some("image/png"));
    assert_eq!(file("report.pdf", 0).mime, "application/pdf");
    assert_eq!(file("noext", 0).mime, "");
}

#[derive(Clone, Default)]
struct Files {
    value: Rc<RefCell<Vec<PickedFile>>>,
    log: Rc<RefCell<Vec<String>>>,
}

impl Files {
    fn value(&self) -> Vec<PickedFile> {
        self.value.borrow().clone()
    }

    fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    fn store(&self) -> impl Fn(Vec<PickedFile>, &mut Window, &mut App) + 'static {
        let (value, log) = (self.value.clone(), self.log.clone());
        move |next, window, _| {
            log.borrow_mut().push(
                next.iter()
                    .map(|f| f.name.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            *value.borrow_mut() = next;
            window.refresh();
        }
    }
}

struct FileHost(Rc<dyn Fn() -> FileInput>);

impl Render for FileHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .p(px(20.))
            .child((self.0)().label_hidden(true).width(px(300.)))
            .child(announcer())
    }
}

/// A 300px field with a hidden label, from y 20: 32px tall in input mode,
/// taller as a dropzone.
fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> FileInput + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| FileHost(Rc::new(build)));
    cx.run_until_parked();
    cx
}

fn choose(cx: &mut VisualTestContext, names: &'static [&'static str]) {
    assert!(cx.did_prompt_for_paths(), "the file dialog opened");
    cx.simulate_path_prompt_response(|_| Some(names.iter().map(PathBuf::from).collect()));
    cx.run_until_parked();
}

#[gpui::test]
fn a_click_opens_the_dialog_and_reports_the_choice(cx: &mut TestAppContext) {
    let files = Files::default();
    let cx = host(cx, {
        let files = files.clone();
        move || FileInput::new("f", "Resume", files.value()).on_change(files.store())
    });
    cx.simulate_click(point(px(100.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    choose(cx, &["resume.pdf"]);
    assert_eq!(files.log(), ["resume.pdf"]);
    let polite = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert_eq!(polite, "1 file selected: resume.pdf");
}

#[gpui::test]
fn a_refused_file_clears_and_shows_why(cx: &mut TestAppContext) {
    let files = Files::default();
    let cx = host(cx, {
        let files = files.clone();
        move || {
            FileInput::new("f", "Photo", files.value())
                .accept("image/*")
                .on_change(files.store())
        }
    });
    cx.simulate_click(point(px(100.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    choose(cx, &["notes.txt"]);
    assert_eq!(files.log(), [""]);
    // The error status announces itself.
    let assertive = cx.update(|window, cx| announcement(Politeness::Assertive, window, cx));
    assert_eq!(assertive, "\"notes.txt\" is not an accepted file type");
}

#[gpui::test]
fn keyboard_opens_it_and_disabled_never_does(cx: &mut TestAppContext) {
    let files = Files::default();
    let disabled = Rc::new(std::cell::Cell::new(false));
    let cx = host(cx, {
        let (files, disabled) = (files.clone(), disabled.clone());
        move || {
            FileInput::new("f", "Resume", files.value())
                .disabled(disabled.get())
                .on_change(files.store())
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    choose(cx, &["a.pdf"]);
    assert_eq!(files.log(), ["a.pdf"]);

    disabled.set(true);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    press(cx, "space");
    cx.simulate_click(point(px(100.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_paths());
}

#[gpui::test]
fn a_dropzone_takes_dropped_files(cx: &mut TestAppContext) {
    let files = Files::default();
    let cx = host(cx, {
        let files = files.clone();
        move || {
            FileInput::new("f", "Photos", files.value())
                .mode(FileInputMode::Dropzone)
                .multiple(true)
                .on_change(files.store())
        }
    });
    let over = point(px(150.), px(50.));
    cx.simulate_event(FileDropEvent::Entered {
        position: over,
        paths: ExternalPaths(["a.png", "b.png"].iter().map(PathBuf::from).collect()),
    });
    cx.simulate_event(FileDropEvent::Pending { position: over });
    cx.simulate_event(FileDropEvent::Submit { position: over });
    cx.run_until_parked();
    assert_eq!(files.log(), ["a.png,b.png"]);
    let polite = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert_eq!(polite, "2 files selected");
}

#[gpui::test]
fn clear_removes_the_files(cx: &mut TestAppContext) {
    let files = Files::default();
    *files.value.borrow_mut() = vec![file("a.pdf", 1)];
    let cx = host(cx, {
        let files = files.clone();
        move || FileInput::new("f", "Resume", files.value()).on_change(files.store())
    });
    cx.simulate_click(point(px(305.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(files.log(), [""]);
    assert!(files.value().is_empty());
    assert!(
        !cx.did_prompt_for_paths(),
        "the clear button does not open the dialog"
    );
}

#[gpui::test]
fn picked_files_read_their_bytes(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("astryx-read-{}.txt", std::process::id()));
    std::fs::write(&path, b"hello astryx").unwrap();
    let file = PickedFile::from_path(&path);
    assert_eq!(file.size, 12);
    assert_eq!(file.mime, "text/plain");
    let task = cx.update(|cx| file.read(cx));
    let bytes = cx.foreground_executor.block_test(task).unwrap();
    assert_eq!(bytes, b"hello astryx");
    std::fs::remove_file(&path).ok();
}
