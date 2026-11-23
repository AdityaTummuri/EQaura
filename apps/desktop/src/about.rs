use crate::ui::Credit;

pub const REPO: &str = "https://github.com/AdityaTummuri/EQaura";

pub fn credits() -> Vec<Credit> {
    let c = |name: &str, license: &str, detail: &str| Credit {
        name: name.into(),
        license: license.into(),
        detail: detail.into(),
    };
    vec![
        c("EQaura", "MIT", "Copyright (c) 2026 Aditya. Source code on GitHub."),
        c("Slint", "Royalty-free License 2.0", "The interface toolkit. slint.dev"),
        c(
            "AutoEq",
            "MIT",
            "Headphone corrections. Copyright (c) 2018-2022 Jaakko Pasanen. Measurements by oratory1990, crinacle, Rtings, Innerfidelity, Headphone.com, HypetheSonics, Super Review and others.",
        ),
        c(
            "Equalizer APO",
            "GPL-2.0",
            "The Windows audio engine by Jonas Thedering. A separate program that EQaura controls through its settings files.",
        ),
        c("PipeWire and WirePlumber", "MIT", "The Linux sound system. Part of your distribution."),
        c("Audio EQ Cookbook", "Public formulas", "Filter formulas by Robert Bristow-Johnson."),
        c(
            "Rust libraries",
            "MIT, Apache-2.0 and similar",
            "See THIRD-PARTY-LICENSES.txt next to the app for the full list.",
        ),
    ]
}
