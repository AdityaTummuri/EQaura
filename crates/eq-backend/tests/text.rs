use eq_backend::{State, apo, pipewire as pw};
use eq_core::filter::{Channel, Coeffs, Filter, Kind};
use eq_core::profile::Profile;

const ID: &str = "{0.0.0.00000000}.{12345678-90ab-cdef-1234-567890abcdef}";

fn sample() -> Profile {
    Profile {
        filters: vec![
            Filter::new(Kind::LowShelf, 105.0, 3.0, 0.7),
            Filter::peak(3200.0, -2.5, 1.4),
            Filter { channel: Channel::Left, ..Filter::peak(900.0, 1.0, 2.0) },
        ],
        ..Profile::default()
    }
}

fn state(p: Profile) -> State {
    State { enabled: true, devices: [(ID.to_owned(), p)].into() }
}

#[test]
fn apo_config_reads_back_as_the_same_sound() {
    let text = apo::config(&state(sample()));
    assert!(text.contains("Device: {12345678-90ab-cdef-1234-567890abcdef}\r\n"));
    assert!(text.ends_with("Device: all\r\n"));
    assert!(!text.contains("GraphicEQ"));
    let parsed = eq_core::preset::parse(&text);
    let chain = sample().chain(48_000.0);
    assert_eq!(parsed.preamp_db, Some(chain.preamp_db));
    let back = Profile { filters: parsed.filters, ..Profile::default() }.chain(48_000.0);
    assert_eq!((back.left, back.right), (chain.left, chain.right));
}

#[test]
fn apo_config_when_off_or_flat() {
    let mut s = state(sample());
    s.enabled = false;
    assert!(!apo::config(&s).contains("Filter"));
    assert!(!apo::config(&state(Profile::default())).contains("Preamp"));
    let mut odd = State { enabled: true, ..State::default() };
    odd.devices.insert("x\r\nInclude: evil.txt".into(), sample());
    assert!(!apo::config(&odd).contains("evil"));
}

#[test]
fn apo_include_keeps_existing_settings() {
    let old = b"\xef\xbb\xbf# mine\r\nPreamp: -6 dB\r\nInclude: example.txt";
    let new = apo::with_include(old);
    assert!(new.starts_with(old));
    assert!(apo::inspect(&new).has_include);
    assert_eq!(apo::with_include(&new), new, "added only once");
    assert_eq!(apo::inspect(old).others.len(), 2);
    assert_eq!(apo::with_include(b""), b"Include: EQaura.txt\r\n");
    assert!(apo::inspect(b"include : eqaura.TXT").has_include);
    assert!(!apo::inspect(b"# Include: EQaura.txt").has_include);
    assert!(apo::inspect(b"\xff\xfe\x00bad").others.is_empty());
}

#[test]
fn pipewire_config_is_complete() {
    let chain = sample().chain(48_000.0);
    let n = pw::slots_for(&chain);
    assert_eq!(n, 24);
    let text =
        pw::config(&pw::slots(&chain, 48_000.0, n), 48_000, &pw::Routing { smart: true, target: None });
    assert_eq!(text.matches("bq_raw").count(), 48);
    assert_eq!(text.matches("output = \"").count(), 46);
    assert!(text.contains("filter.smart = true") && !text.contains("target.object"));
    assert_eq!(text.matches('{').count(), text.matches('}').count());
    assert_eq!(text.matches('[').count(), text.matches(']').count());
    assert!(!text.contains("NaN") && !text.contains("inf"));
    let hostile = pw::Routing { smart: false, target: Some("a\" } evil = { x = \"\n".into()) };
    let text = pw::config(&pw::slots(&chain, 48_000.0, n), 48_000, &hostile);
    assert!(text.contains(r#"target.object = "a\" } evil = { x = \"""#));
}

#[test]
fn pipewire_live_updates_are_small_and_exact() {
    let a = pw::slots(&sample().chain(48_000.0), 48_000.0, 24);
    assert!(pw::live_updates(&a, &a).is_empty());
    let mut p = sample();
    p.filters[1].gain_db = -3.0;
    let b = pw::slots(&p.chain(48_000.0), 48_000.0, 24);
    let u = pw::live_updates(&a, &b);
    assert_eq!(u.len(), 1);
    assert!(u[0].contains("\"l2:b0\"") && u[0].contains("\"r3:a2\"") || u[0].contains("\"r2:a2\""));
    let all = pw::live_updates(&vec![(Coeffs::IDENTITY, Coeffs::IDENTITY); 24], &b);
    for m in &all {
        // pod size: 16 bytes per name, 16 per value, plus small headers
        assert!(m.matches(':').count() * 32 + 64 < 1024, "{m}");
    }
}

#[test]
fn pipewire_preamp_and_extremes() {
    let p = Profile { filters: vec![Filter::peak(1000.0, 6.0, 1.0); 40], ..Profile::default() };
    let chain = p.chain(48_000.0);
    let n = pw::slots_for(&chain);
    assert!(n >= 41 && n.is_multiple_of(8));
    let s = pw::slots(&chain, 48_000.0, n);
    assert!((20.0 * s[0].0.b0.log10() - chain.preamp_db).abs() < 1e-9);
    let text = pw::config(&s, 48_000, &pw::Routing { smart: false, target: None });
    let exponent = |w: &str| w.parse::<f64>().is_ok() && w.contains(['e', 'E']);
    assert!(!text.split_whitespace().any(exponent));
}

#[test]
fn reads_pipewire_output() {
    let dump = r#"[
      {"id":31,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"alsa.usb","node.description":"USB DAC"}}},
      {"id":32,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"eqaura.sink"}}},
      {"id":33,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"mic"}}},
      {"id":34,"type":"PipeWire:Interface:Node","info":null},
      {"id":40,"type":"PipeWire:Interface:Metadata","metadata":[{"key":"default.audio.sink","value":{"name":"alsa.usb"}},{"key":"clock.rate","value":44100},{"key":"clock.force-rate","value":0}]}
    ]"#;
    let g = pw::read_dump(dump);
    assert_eq!(g.sinks.len(), 2);
    assert_eq!(g.id_of("eqaura.sink"), Some(32));
    assert_eq!(g.default_sink.as_deref(), Some("alsa.usb"));
    assert_eq!(g.rate, Some(44_100));
    assert_eq!(pw::read_dump("nonsense"), pw::Graph::default());
    assert_eq!(pw::read_version("pipewire\nCompiled with libpipewire 1.0.5\n"), Some((1, 0, 5)));
    assert_eq!(pw::read_version("wireplumber\nCompiled with libwireplumber 0.4.17"), Some((0, 4, 17)));
    assert_eq!(pw::read_version("none"), None);
}

#[test]
fn unit_file_escapes_paths() {
    let u = pw::unit("/home/a b/100%/x.conf");
    assert!(u.contains(r#"pipewire -c "/home/a b/100%%/x.conf""#));
}
