use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const LOG_BYTES: u64 = 1024 * 1024;
const LOG_FILES: u32 = 5;

pub struct Dir {
    path: PathBuf,
}

/// Writes beside the target and then swaps it in, so a crash or a full disk
/// never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let write = || -> io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A file name that is safe on every system.
pub fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .take(60)
        .collect();
    let s = s.trim().to_owned();
    const RESERVED: [&str; 6] = ["con", "prn", "aux", "nul", "com", "lpt"];
    let lower = s.to_lowercase();
    let reserved = RESERVED.iter().any(|r| lower == *r || (lower.len() == 4 && lower.starts_with(r)));
    if s.is_empty() {
        "preset".into()
    } else if reserved {
        format!("_{s}")
    } else {
        s
    }
}

impl Dir {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A damaged file is kept as `.broken` instead of being overwritten.
    pub fn read_json<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let path = self.path.join(name);
        let text = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&text) {
            Ok(v) => Some(v),
            Err(_) => {
                let _ = std::fs::rename(&path, path.with_extension("broken"));
                None
            }
        }
    }

    pub fn write_json<T: Serialize>(&self, name: &str, value: &T) -> io::Result<()> {
        let text = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
        write_atomic(&self.path.join(name), &text)
    }

    pub fn list(&self, sub: &str, ext: &str) -> Vec<PathBuf> {
        let Ok(dir) = std::fs::read_dir(self.path.join(sub)) else { return Vec::new() };
        let mut out: Vec<PathBuf> =
            dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect();
        out.sort();
        out
    }

    pub fn free_name(&self, sub: &str, name: &str, ext: &str) -> PathBuf {
        let base = safe_name(name);
        let dir = self.path.join(sub);
        let mut path = dir.join(format!("{base}.{ext}"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{base} {n}.{ext}"));
            n += 1;
        }
        path
    }

    pub fn log(&self, line: &str) {
        let path = self.path.join("eqaura.log");
        let numbered = |n: u32| self.path.join(format!("eqaura.{n}.log"));
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_BYTES) {
            for n in (1..LOG_FILES - 1).rev() {
                let _ = std::fs::rename(numbered(n), numbered(n + 1));
            }
            let _ = std::fs::rename(&path, numbered(1));
        }
        let _ = std::fs::create_dir_all(&self.path);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = writeln!(f, "{secs} {}", line.replace(['\n', '\r'], " "));
        }
    }
}
