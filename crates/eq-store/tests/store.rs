use eq_core::{Filter, Profile};
use eq_store::*;

fn temp(name: &str) -> Store {
    let p = std::env::temp_dir().join(format!("eqaura-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    Store::at(p)
}

#[test]
fn settings_round_trip_and_survive_damage() {
    let s = temp("settings");
    assert_eq!(s.settings(), Settings::default());
    let mut v = Settings { theme: Theme::Light, ai: AiChoice::Ollama, ..Settings::default() };
    v.ai_settings.insert("ollama".into(), AiSettings { url: "u".into(), model: "m".into() });
    s.save_settings(&v).unwrap();
    assert_eq!(s.settings(), v);
    std::fs::write(s.path().join("settings.json"), "{ broken").unwrap();
    assert_eq!(s.settings(), Settings::default());
    assert!(s.path().join("settings.broken").exists());
    std::fs::write(s.path().join("settings.json"), r#"{"theme":"dark","future":1}"#).unwrap();
    assert_eq!(s.settings().theme, Theme::Dark);
}

#[test]
fn presets_save_rename_delete() {
    let s = temp("presets");
    let p = |n: &str| Profile { name: n.into(), filters: vec![Filter::peak(100.0, 1.0, 1.0)], ..Profile::default() };
    for n in ["Warm", "a/b:c", "a_b_c", "../../escape", "NUL", "音楽"] {
        s.save_preset(&p(n)).unwrap();
    }
    s.save_preset(&Profile { filters: vec![], ..p("Warm") }).unwrap();
    let names: Vec<String> = s.presets().into_iter().map(|p| p.name).collect();
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(s.presets().iter().find(|p| p.name == "Warm").unwrap().filters.is_empty());
    s.rename_preset("Warm", "Cosy").unwrap();
    s.delete_preset("a/b:c").unwrap();
    let names: Vec<String> = s.presets().into_iter().map(|p| p.name).collect();
    assert!(names.contains(&"Cosy".into()) && !names.contains(&"Warm".into()));
    assert!(names.contains(&"a_b_c".into()) && !names.contains(&"a/b:c".into()));
    assert!(!s.path().parent().unwrap().join("escape.json").exists());
}

#[test]
fn log_rotates() {
    let s = temp("log");
    let line = "x".repeat(200_000);
    for _ in 0..40 {
        s.log(&line);
    }
    let logs = std::fs::read_dir(s.path()).unwrap().count();
    assert!(logs <= 5, "{logs} log files");
}
