//! PipeWire filter-chain configuration and live updates.
//!
//! Every band is a `bq_raw` node whose coefficients EQaura computes, because
//! PipeWire's own shelves ignore Q. Coefficients can be changed while sound
//! plays, so nothing restarts unless more nodes are needed.

use eq_core::filter::Coeffs;
use eq_core::profile::Chain;
use std::fmt::Write;

pub const SINK: &str = "eqaura.sink";
pub const OUTPUT: &str = "eqaura.output";
const MIN_SLOTS: usize = 24;

/// Nodes needed per ear: one for the preamp, one per filter, rounded up so
/// adding a band rarely needs a restart.
pub fn slots_for(chain: &Chain) -> usize {
    let need = 1 + chain.left.len().max(chain.right.len());
    need.max(MIN_SLOTS).div_ceil(8) * 8
}

/// Coefficients per slot for one ear.
fn line(filters: &[eq_core::Filter], preamp_db: f64, rate: f64, slots: usize) -> Vec<Coeffs> {
    let mut out = vec![Coeffs { b0: 10f64.powf(preamp_db / 20.0), ..Coeffs::IDENTITY }];
    out.extend(filters.iter().map(|f| f.coeffs(rate)));
    out.resize(slots.max(out.len()), Coeffs::IDENTITY);
    out
}

pub type Slots = Vec<(Coeffs, Coeffs)>;

pub fn slots(chain: &Chain, rate: f64, count: usize) -> Slots {
    line(&chain.left, chain.preamp_db, rate, count)
        .into_iter()
        .zip(line(&chain.right, chain.preamp_db, rate, count))
        .collect()
}

/// Full precision without exponents, which SPA-JSON reads reliably.
fn num(v: f64) -> String {
    let s = format!("{v:.12}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0.0".into() } else if s.contains('.') { s.into() } else { format!("{s}.0") }
}

fn controls(c: &Coeffs, prefix: &str, sep: &str) -> String {
    [("b0", c.b0), ("b1", c.b1), ("b2", c.b2), ("a1", c.a1), ("a2", c.a2)]
        .iter()
        .map(|(k, v)| format!("\"{prefix}{k}\"{sep}{}", num(*v)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Command arguments that bring the running chain from `old` to `new`. Each
/// message carries two bands, which fits pw-cli's 1024-byte limit.
pub fn live_updates(old: &Slots, new: &Slots) -> Vec<String> {
    let changed: Vec<usize> = (0..new.len()).filter(|i| old.get(*i) != new.get(*i)).collect();
    changed
        .chunks(2)
        .map(|chunk| {
            let mut s = String::from("{ params = [");
            for i in chunk {
                let (l, r) = &new[*i];
                let _ = write!(s, " {} {}", controls(l, &format!("l{i}:"), " "), controls(r, &format!("r{i}:"), " "));
            }
            s + " ] }"
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routing {
    /// WirePlumber 0.5 and newer can insert the filter in front of whichever
    /// device is the default, by itself.
    pub smart: bool,
    /// Device to play to when smart routing is not available.
    pub target: Option<String>,
}

/// Node names come from the system; keep them from ending the string early.
fn quoted(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", clean.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A complete configuration for `pipewire -c`.
pub fn config(slots: &Slots, rate: u32, routing: &Routing) -> String {
    let mut nodes = String::new();
    let mut links = String::new();
    for (side, pick) in [("l", 0), ("r", 1)] {
        for (i, pair) in slots.iter().enumerate() {
            let c = if pick == 0 { &pair.0 } else { &pair.1 };
            // `config` gives the starting coefficients, `control` the same
            // values for versions that let controls win.
            let _ = writeln!(
                nodes,
                "          {{ type = builtin name = {side}{i} label = bq_raw config = {{ coefficients = [ {{ rate = {rate} b0 = {} b1 = {} b2 = {} a0 = 1.0 a1 = {} a2 = {} }} ] }} }}",
                num(c.b0), num(c.b1), num(c.b2), num(c.a1), num(c.a2)
            );
            if i > 0 {
                let _ = writeln!(
                    links,
                    "          {{ output = \"{side}{}:Out\" input = \"{side}{i}:In\" }}",
                    i - 1
                );
            }
        }
    }
    let last = slots.len().saturating_sub(1);
    let smart = if routing.smart {
        "\n        filter.smart = true\n        filter.smart.name = eqaura"
    } else {
        ""
    };
    let target = match (&routing.target, routing.smart) {
        (Some(t), false) => format!("\n        target.object = {}", quoted(t)),
        _ => String::new(),
    };
    format!(
        r#"# Written by EQaura. Changes made here are replaced.
context.properties = {{
  log.level = 1
}}
context.spa-libs = {{
  audio.convert.* = audioconvert/libspa-audioconvert
  support.* = support/libspa-support
}}
context.modules = [
  {{ name = libpipewire-module-rt args = {{ }} flags = [ ifexists nofail ] }}
  {{ name = libpipewire-module-protocol-native }}
  {{ name = libpipewire-module-client-node }}
  {{ name = libpipewire-module-adapter }}
  {{ name = libpipewire-module-filter-chain
    args = {{
      node.description = "EQaura"
      media.name = "EQaura"
      audio.rate = {rate}
      audio.channels = 2
      audio.position = [ FL FR ]
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
        inputs = [ "l0:In" "r0:In" ]
        outputs = [ "l{last}:Out" "r{last}:Out" ]
      }}
      capture.props = {{
        node.name = {SINK}
        media.class = Audio/Sink
        node.virtual = true{smart}
      }}
      playback.props = {{
        node.name = {OUTPUT}
        node.passive = true{target}
      }}
    }}
  }}
]
"#
    )
}

/// systemd user unit. It is a separate process, so PipeWire never restarts.
pub fn unit(config_path: &str) -> String {
    // "%" starts a specifier in unit files.
    let path = config_path.replace('%', "%%").replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[Unit]\nDescription=EQaura equalizer\nAfter=pipewire.service wireplumber.service\nPartOf=pipewire.service\n\n\
         [Service]\nType=simple\nExecStart=/usr/bin/env pipewire -c \"{path}\"\nRestart=on-failure\nRestartSec=2\n\n\
         [Install]\nWantedBy=pipewire.service\n"
    )
}

/// What `pw-dump` says about the running system.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    /// (object id, node name, description)
    pub sinks: Vec<(u64, String, String)>,
    pub default_sink: Option<String>,
    pub rate: Option<u32>,
}

impl Graph {
    pub fn id_of(&self, name: &str) -> Option<u64> {
        self.sinks.iter().find(|s| s.1 == name).map(|s| s.0)
    }
}

pub fn read_dump(json: &str) -> Graph {
    let mut g = Graph::default();
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(json) else { return g };
    for item in &items {
        let props = item.pointer("/info/props");
        let text = |k: &str| props.and_then(|p| p.get(k)).and_then(|v| v.as_str());
        match item.get("type").and_then(|t| t.as_str()) {
            Some("PipeWire:Interface:Node") if text("media.class") == Some("Audio/Sink") => {
                if let (Some(id), Some(name)) = (item.get("id").and_then(|i| i.as_u64()), text("node.name")) {
                    let label = text("node.description").or(text("node.nick")).unwrap_or(name);
                    g.sinks.push((id, name.into(), label.into()));
                }
            }
            Some("PipeWire:Interface:Metadata") => {
                for m in item.get("metadata").and_then(|m| m.as_array()).into_iter().flatten() {
                    let key = m.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    let value = m.get("value");
                    match key {
                        "default.audio.sink" => {
                            g.default_sink =
                                value.and_then(|v| v.get("name")).and_then(|n| n.as_str()).map(str::to_owned);
                        }
                        "clock.rate" => {
                            g.rate = value.and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok());
                        }
                        "clock.force-rate" => {
                            if let Some(v) = value.and_then(|v| v.as_u64()).filter(|v| *v > 0) {
                                g.rate = u32::try_from(v).ok();
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    g
}

/// The first "x.y.z" in a command's version output.
pub fn read_version(text: &str) -> Option<(u32, u32, u32)> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.')).find_map(|word| {
        let mut p = word.split('.').map(|n| n.parse::<u32>().ok());
        match (p.next()??, p.next()??, p.next()) {
            (a, b, c) => Some((a, b, c.flatten().unwrap_or(0))),
        }
    })
}
