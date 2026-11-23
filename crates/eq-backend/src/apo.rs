//! Equalizer APO configuration text. Only parametric `Filter:` lines are
//! written, never `GraphicEQ`.

use crate::State;
use eq_core::filter::{Channel, Filter};
use eq_core::preset;
use eq_core::profile::Chain;

pub const FILE: &str = "EQaura.txt";
pub const INCLUDE: &str = "Include: EQaura.txt";

/// The `{...}` at the end of a Windows endpoint id, which Equalizer APO
/// matches exactly.
pub fn endpoint_guid(id: &str) -> Option<&str> {
    let start = id.rfind('{')?;
    let guid = &id[start..];
    let inner = guid.strip_prefix('{')?.strip_suffix('}')?;
    let ok = inner.len() == 36 && inner.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    ok.then_some(guid)
}

fn chain_text(chain: &Chain) -> String {
    if chain.is_symmetric() {
        return preset::write(chain.preamp_db, &chain.left);
    }
    let side = |v: &[Filter], channel| v.iter().map(move |f| Filter { channel, ..*f }).collect::<Vec<_>>();
    let mut all = side(&chain.left, Channel::Left);
    all.extend(side(&chain.right, Channel::Right));
    preset::write(chain.preamp_db, &all)
}

pub fn config(state: &State) -> String {
    let mut s = String::from("# Written by EQaura. Changes made here are replaced.\r\n");
    if !state.enabled {
        s += "# EQaura is off.\r\n";
        return s;
    }
    for (id, profile) in &state.devices {
        let Some(guid) = endpoint_guid(id) else { continue };
        let chain = profile.chain(48_000.0);
        if chain.left.is_empty() && chain.right.is_empty() && chain.preamp_db == 0.0 {
            continue;
        }
        s += &format!("Device: {guid}\r\n{}", chain_text(&chain).replace('\n', "\r\n"));
    }
    s += "Device: all\r\n";
    s
}

/// What Equalizer APO's own `config.txt` contains.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Main {
    pub has_include: bool,
    /// Other active settings, which change the sound on top of EQaura.
    pub others: Vec<String>,
}

fn is_include(line: &str) -> bool {
    line.split_once(':').is_some_and(|(k, v)| {
        k.trim().eq_ignore_ascii_case("include") && v.trim().eq_ignore_ascii_case(FILE)
    })
}

pub fn inspect(bytes: &[u8]) -> Main {
    let text = String::from_utf8_lossy(bytes);
    let mut m = Main::default();
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || !line.contains(':') {
            continue;
        }
        if is_include(line) {
            m.has_include = true;
        } else {
            m.others.push(line.into());
        }
    }
    m
}

/// Adds our include line and keeps every byte that was there.
pub fn with_include(bytes: &[u8]) -> Vec<u8> {
    if inspect(bytes).has_include {
        return bytes.to_vec();
    }
    let mut out = bytes.to_vec();
    if !out.is_empty() && !out.ends_with(b"\n") {
        out.extend_from_slice(b"\r\n");
    }
    // Earlier Device or Channel lines must not narrow our settings.
    if !inspect(bytes).others.is_empty() {
        out.extend_from_slice(b"Device: all\r\nChannel: all\r\n");
    }
    out.extend_from_slice(INCLUDE.as_bytes());
    out.extend_from_slice(b"\r\n");
    out
}
