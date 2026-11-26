//! Remembered UI state, upstream's `localStorage` use (Resizable's `autoSaveId`).
//!
//! The browser keeps entries in `localStorage`. A native app keeps them in a JSON file
//! named after its executable, in the platform's data directory
//! (`%APPDATA%\astryx`, `~/Library/Application Support/astryx`, or
//! `$XDG_DATA_HOME/astryx`). An app can replace the store with `set_storage`; tests use
//! `use_memory_storage`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui::{App, Global};

/// Where remembered entries live.
pub trait StorageBackend {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
}

struct Storage(Rc<dyn StorageBackend>);

impl Global for Storage {}

/// Replaces the store.
pub fn set_storage(cx: &mut App, backend: impl StorageBackend + 'static) {
    cx.set_global(Storage(Rc::new(backend)));
}

/// A store that forgets everything when the app ends.
pub fn use_memory_storage(cx: &mut App) {
    set_storage(cx, MemoryStorage::default());
}

fn backend(cx: &mut App) -> Rc<dyn StorageBackend> {
    if let Some(Storage(backend)) = cx.try_global::<Storage>() {
        return backend.clone();
    }
    let backend: Rc<dyn StorageBackend> = Rc::new(PlatformStorage::default());
    cx.set_global(Storage(backend.clone()));
    backend
}

/// The entry under `key`.
pub fn read(cx: &mut App, key: &str) -> Option<String> {
    backend(cx).get(key)
}

/// Remembers `value` under `key`.
pub fn write(cx: &mut App, key: &str, value: &str) {
    backend(cx).set(key, value);
}

/// An in-memory store.
#[derive(Default)]
pub struct MemoryStorage(RefCell<BTreeMap<String, String>>);

impl StorageBackend for MemoryStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.0.borrow().get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) {
        self.0
            .borrow_mut()
            .insert(key.to_string(), value.to_string());
    }
}

/// The platform's store: `localStorage` in a browser, a JSON file natively.
#[derive(Default)]
struct PlatformStorage {
    #[cfg(not(target_family = "wasm"))]
    entries: RefCell<Option<BTreeMap<String, String>>>,
}

#[cfg(target_family = "wasm")]
impl StorageBackend for PlatformStorage {
    fn get(&self, key: &str) -> Option<String> {
        web_sys::window()?
            .local_storage()
            .ok()??
            .get_item(key)
            .ok()?
    }

    fn set(&self, key: &str, value: &str) {
        if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
            storage.set_item(key, value).ok();
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl PlatformStorage {
    fn path() -> Option<std::path::PathBuf> {
        use std::path::PathBuf;
        let base = if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
                })
        }?;
        let app = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "app".to_string());
        Some(base.join("astryx").join(format!("{app}.json")))
    }

    fn with_entries<R>(&self, f: impl FnOnce(&mut BTreeMap<String, String>) -> R) -> R {
        let mut entries = self.entries.borrow_mut();
        let entries = entries.get_or_insert_with(|| {
            Self::path()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default()
        });
        f(entries)
    }
}

#[cfg(not(target_family = "wasm"))]
impl StorageBackend for PlatformStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.with_entries(|entries| entries.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) {
        let text = self.with_entries(|entries| {
            entries.insert(key.to_string(), value.to_string());
            serde_json::to_string_pretty(entries).ok()
        });
        if let (Some(path), Some(text)) = (Self::path(), text) {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            std::fs::write(path, text).ok();
        }
    }
}
