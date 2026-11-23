//! API keys live in the system key store: Keychain on macOS, Credential
//! Manager on Windows, Secret Service on Linux. When none is available the key
//! is kept in memory until the app closes.

use std::collections::HashMap;
use std::sync::Mutex;

static SESSION: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    System,
    SessionOnly,
}

pub fn store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else {
        "system keyring"
    }
}

fn entry(provider: &str) -> Option<keyring::Entry> {
    // Tests and demo runs must never touch the real key store.
    if std::env::var_os("EQAURA_HOME").is_some() {
        return None;
    }
    keyring::Entry::new("EQaura", provider).ok()
}

fn session<R>(f: impl FnOnce(&mut HashMap<String, String>) -> R) -> R {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

pub fn set(provider: &str, key: &str) -> Saved {
    if entry(provider).is_some_and(|e| e.set_password(key).is_ok()) {
        session(|s| s.remove(provider));
        Saved::System
    } else {
        session(|s| s.insert(provider.into(), key.into()));
        Saved::SessionOnly
    }
}

pub fn get(provider: &str) -> Option<String> {
    session(|s| s.get(provider).cloned())
        .or_else(|| entry(provider).and_then(|e| e.get_password().ok()))
        .filter(|k| !k.is_empty())
}

pub fn clear(provider: &str) {
    session(|s| s.remove(provider));
    if let Some(e) = entry(provider) {
        let _ = e.delete_credential();
    }
}
