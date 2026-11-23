//! Number parsing and formatting shared by every screen.

/// Accepts "1200", "1.2k", "1,2 kHz", "1 200 Hz", "-3 dB", "+2.5".
pub fn parse_number(input: &str) -> Option<f64> {
    let mut s: String = input
        .trim()
        .to_lowercase()
        .replace('\u{2212}', "-")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
        .collect();
    let mut scale = 1.0;
    for unit in ["khz", "hz", "db", "k"] {
        if let Some(rest) = s.strip_suffix(unit) {
            if unit.starts_with('k') {
                scale = 1000.0;
            }
            s = rest.to_owned();
            break;
        }
    }
    if s.contains(',') && s.contains('.') {
        s = s.replace(',', "");
    } else if let Some((head, tail)) = s.split_once(',') {
        // "1,200" is a thousands separator, "1,2" is a decimal comma.
        let thousands = tail.len() == 3
            && !head.trim_start_matches(['-', '+']).starts_with('0')
            && tail.bytes().all(|b| b.is_ascii_digit())
            && scale == 1.0;
        s = if thousands { s.replace(',', "") } else { s.replace(',', ".") };
    }
    // Rust also parses "inf" and "nan"; neither is a usable setting.
    s.parse::<f64>().ok().map(|v| v * scale).filter(|v| v.is_finite())
}

fn trim(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_owned() } else { s };
    if s == "-0" { "0".into() } else { s }
}

pub fn freq(hz: f64) -> String {
    if hz >= 1000.0 {
        format!("{} kHz", trim(hz / 1000.0, 2))
    } else if hz >= 100.0 {
        format!("{} Hz", trim(hz, 0))
    } else {
        format!("{} Hz", trim(hz, 1))
    }
}

/// Signed, with a real minus sign: "+3 dB", "−2.5 dB", "0 dB".
pub fn gain(db: f64) -> String {
    format!("{} dB", signed(db))
}

pub fn signed(db: f64) -> String {
    let s = trim(db.abs(), 1);
    if s == "0" {
        s
    } else if db < 0.0 {
        format!("\u{2212}{s}")
    } else {
        format!("+{s}")
    }
}

pub fn q(v: f64) -> String {
    trim(v, 2)
}
