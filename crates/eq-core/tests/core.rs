use eq_core::dsp::Stereo;
use eq_core::filter::{Channel, Coeffs, Filter, Kind};
use eq_core::profile::{Correction, HearingLayer, Preamp, Profile};
use eq_core::{autoeq, change, curve, hearing, history::History, preset, rules, sliders, text, warning};
use proptest::prelude::*;

const FS: f64 = 48_000.0;

/// Independent evaluation of the analogue prototype after the bilinear
/// transform, used as the reference for the cookbook coefficients.
fn analog_db(f: &Filter, freq: f64) -> f64 {
    let w = (std::f64::consts::PI * freq / FS).tan() / (std::f64::consts::PI * f.freq_hz / FS).tan();
    let a = 10f64.powf(f.gain_db / 40.0);
    let q = f.q;
    // |num|^2 / |den|^2 for s = jw
    let c = |re: f64, im: f64| re * re + im * im;
    let r = match f.kind {
        Kind::Peaking => c(1.0 - w * w, w * a / q) / c(1.0 - w * w, w / (a * q)),
        Kind::LowShelf => a * a * c(a - w * w, w * a.sqrt() / q) / c(1.0 - a * w * w, w * a.sqrt() / q),
        Kind::HighShelf => a * a * c(1.0 - a * w * w, w * a.sqrt() / q) / c(a - w * w, w * a.sqrt() / q),
        Kind::LowPass => 1.0 / c(1.0 - w * w, w / q),
        Kind::HighPass => (w * w) * (w * w) / c(1.0 - w * w, w / q),
        Kind::Notch => c(1.0 - w * w, 0.0) / c(1.0 - w * w, w / q),
    };
    10.0 * r.max(1e-30).log10()
}

#[test]
fn responses_match_the_analogue_reference() {
    for kind in Kind::ALL {
        for gain in [-24.0, -6.0, 3.0, 12.0, 30.0] {
            for q in [0.1, 0.71, 1.41, 10.0] {
                for fc in [20.0, 105.0, 1000.0, 9187.0, 20_000.0] {
                    let f = Filter::new(kind, fc, gain, q);
                    for x in curve::log_grid(20.0, 20_000.0, 97) {
                        let got = f.coeffs(FS).gain_db_at(x, FS);
                        let want = analog_db(&f, x);
                        if want > -80.0 {
                            assert!((got - want).abs() < 0.01, "{f:?} at {x}: {got} vs {want}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn processed_audio_matches_the_drawn_curve() {
    let p = Profile {
        filters: vec![
            Filter::new(Kind::LowShelf, 105.0, 5.5, 0.7),
            Filter::peak(3200.0, -4.0, 1.4),
            Filter { channel: Channel::Left, ..Filter::peak(900.0, 6.0, 2.0) },
            Filter::new(Kind::HighShelf, 9000.0, 3.0, 0.7),
        ],
        ..Profile::default()
    };
    let chain = p.chain(FS);
    for freq in [40.0, 105.0, 900.0, 3200.0, 12_000.0] {
        let mut s = Stereo::new(&chain, FS);
        let n = 96_000;
        let tone: Vec<f64> =
            (0..n).map(|i| 0.1 * (2.0 * std::f64::consts::PI * freq * i as f64 / FS).sin()).collect();
        for (line, filters) in [(&mut s.left, &chain.left), (&mut s.right, &chain.right)] {
            let mut buf = tone.clone();
            for block in buf.chunks_mut(512) {
                line.process(block);
            }
            let rms = |b: &[f64]| (b.iter().map(|x| x * x).sum::<f64>() / b.len() as f64).sqrt();
            let measured = 20.0 * (rms(&buf[n / 2..]) / rms(&tone[n / 2..])).log10();
            let drawn = curve::response_at(filters, freq, FS) + chain.preamp_db;
            assert!((measured - drawn).abs() < 0.02, "{freq} Hz: {measured} vs {drawn}");
        }
    }
}

#[test]
fn strange_sample_rates_and_frequencies_never_break() {
    for kind in Kind::ALL {
        for fc in [0.001, 5.0, 24_000.0, 40_000.0, 1e9] {
            for rate in [0.0, -1.0, f64::NAN, 8000.0, 44_100.0, 192_000.0] {
                assert!(Filter::new(kind, fc, 12.0, 1.0).coeffs(rate).is_stable());
            }
        }
    }
    assert_eq!(Filter::peak(f64::NAN, 1.0, 1.0).coeffs(FS), Coeffs::IDENTITY);
    assert_eq!(Filter::peak(100.0, 1.0, 0.0).coeffs(FS), Coeffs::IDENTITY);
}

#[test]
fn bad_input_samples_do_not_poison_the_filter() {
    let p = Profile { filters: vec![Filter::peak(1000.0, 6.0, 1.0)], ..Profile::default() };
    let mut s = Stereo::new(&p.chain(FS), FS);
    s.left.process(&mut [f64::NAN, 1.0, f64::INFINITY]);
    let mut ok = [0.5; 64];
    s.left.process(&mut ok);
    assert!(ok.iter().all(|x| x.is_finite()));
}

fn any_filter() -> impl Strategy<Value = Filter> {
    (0..6usize, 1.0f64..30_000.0, -40.0f64..40.0, 0.05f64..60.0, any::<bool>(), 0..3usize).prop_map(
        |(k, f, g, q, on, c)| Filter { on, channel: Channel::ALL[c], ..Filter::new(Kind::ALL[k], f, g, q) },
    )
}

fn any_profile() -> impl Strategy<Value = Profile> {
    (
        prop::collection::vec(any_filter(), 0..30),
        prop::option::of((prop::collection::vec(any_filter(), 0..10), 0.0f64..1.0)),
        prop::option::of((
            prop::collection::vec(any_filter(), 0..8),
            prop::collection::vec(any_filter(), 0..8),
        )),
    )
        .prop_map(|(filters, hp, hl)| Profile {
            filters,
            headphone: hp.map(|(filters, strength)| Correction {
                name: "x".into(),
                source: String::new(),
                on: true,
                strength,
                filters,
            }),
            hearing: hl.map(|(left, right)| HearingLayer { on: true, strength: 0.7, left, right }),
            ..Profile::default()
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000 / if cfg!(debug_assertions) { 20 } else { 1 }))]

    #[test]
    fn auto_preamp_never_lets_the_curve_above_zero(p in any_profile()) {
        let chain = p.chain(FS);
        let mut grid = curve::log_grid(2.0, 23_990.0, 1500);
        grid.extend(chain.left.iter().chain(&chain.right).map(|f| f.freq_hz).filter(|f| *f < 23_990.0));
        for filters in [&chain.left, &chain.right] {
            for f in &grid {
                let total = curve::response_at(filters, *f, FS) + chain.preamp_db;
                prop_assert!(total <= 0.02, "{} dB at {} Hz", total, f);
            }
        }
    }

    #[test]
    fn every_filter_is_stable(f in any_filter(), rate in 8000.0f64..384_000.0) {
        prop_assert!(f.coeffs(rate).is_stable());
    }

    #[test]
    fn profiles_survive_json(p in any_profile()) {
        let back = Profile::from_json(&p.to_json()).unwrap();
        prop_assert_eq!(back.filters.len(), p.filters.len());
        prop_assert_eq!(back.to_json(), p.to_json());
    }

    #[test]
    fn preset_text_round_trips(filters in prop::collection::vec(any_filter(), 0..20)) {
        let parsed = preset::parse(&preset::write(-3.5, &filters));
        prop_assert!(parsed.skipped.is_empty());
        prop_assert_eq!(parsed.preamp_db, Some(-3.5));
        prop_assert_eq!(parsed.filters.len(), filters.len());
        let mut sorted = filters.clone();
        sorted.sort_by_key(|f| f.channel as u8);
        for (a, b) in parsed.filters.iter().zip(&sorted) {
            prop_assert_eq!(a.kind, b.kind);
            prop_assert_eq!(a.channel, b.channel);
            prop_assert_eq!(a.on, b.on);
            prop_assert!((a.freq_hz - b.freq_hz).abs() < 0.01);
            prop_assert!(!a.kind.has_gain() || (a.gain_db - b.gain_db).abs() < 0.01);
            prop_assert!((a.q - b.q).abs() < 0.001);
        }
    }

    #[test]
    fn parsers_never_panic(s in "\\PC*", lines in prop::collection::vec("(Preamp|Filter 1|Channel|GraphicEQ): (ON|OFF)? ?(PK|LSC|LS|BW|Oct|Fc|Gain|Q|[0-9.,eE+-]{1,9}| |dB|Hz|kHz){0,12}", 0..12)) {
        for f in preset::parse(&s).filters.iter().chain(&preset::parse(&lines.join("\n")).filters) {
            prop_assert!(f.is_valid());
        }
        let _ = Profile::from_json(&s);
        let _ = text::parse_number(&s);
        let _ = rules::suggest(&s, &[]);
        let _ = autoeq::search(&s);
    }
}

#[test]
fn reads_autoeq_and_other_dialects() {
    let t = "\u{feff}Preamp: -6.1 dB\r\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70\r\nFilter 2: OFF PK Fc 1.2 kHz Gain -3 dB Q 1.41\nFilter: ON NO Fc 50 Hz\nFilter 4: ON None\nChannel: L\nFilter 5: ON HP Fc 30 Hz\nChannel: L R\nFilter 6: ON PK Fc 2000 Hz Gain 2 dB BW Oct 1\nFilter 7: ON LS 6 dB Fc 100 Hz Gain 4 dB\nGraphicEQ: 20 0; 30 1\n# note";
    let p = preset::parse(t);
    assert_eq!(p.preamp_db, Some(-6.1));
    assert_eq!(p.filters.len(), 6);
    assert_eq!(p.filters[0], Filter::new(Kind::LowShelf, 105.0, 5.5, 0.7));
    assert!(!p.filters[1].on && p.filters[1].freq_hz == 1200.0);
    assert_eq!(p.filters[3].channel, Channel::Left);
    assert_eq!(p.filters[4].channel, Channel::Both);
    assert!((p.filters[4].q - 1.414).abs() < 0.01);
    assert!(p.filters[5].freq_hz > 100.0);
    assert_eq!(p.skipped, ["GraphicEQ: 20 0; 30 1"]);
}

#[test]
fn autoeq_preamp_becomes_automatic() {
    let e = autoeq::get(autoeq::search("Sennheiser HD 600")[0]).unwrap();
    let chain = Profile { filters: e.filters(), ..Profile::default() }.chain(FS);
    let text = preset::write(chain.preamp_db, &e.filters());
    assert!(preset::parse(&text).into_profile("x").preamp.auto);
    let louder = preset::parse("Preamp: 3 dB\nFilter 1: ON PK Fc 100 Hz Gain 3 dB Q 1").into_profile("x");
    assert_eq!(louder.preamp, Preamp { auto: false, db: 3.0 });
}

#[test]
fn headphone_search_is_forgiving() {
    assert_eq!(autoeq::len(), 8850);
    for q in ["hd600", "HD 600", "sennheiser hd600", "hd 600 senn"] {
        let hit = autoeq::get(autoeq::search(q)[0]).unwrap();
        assert!(hit.name.contains("HD 600"), "{q} found {}", hit.name);
    }
    assert_eq!(autoeq::search("").len(), autoeq::len());
    assert!(autoeq::search("zzzz not a headphone").is_empty());
    for i in 0..autoeq::len() {
        assert!(!autoeq::get(i).unwrap().filters().is_empty());
    }
}

#[test]
fn layers_and_channels_reach_the_right_ear() {
    let p = Profile {
        filters: vec![Filter { channel: Channel::Right, ..Filter::peak(1000.0, 3.0, 1.0) }],
        headphone: Some(Correction {
            name: "h".into(),
            source: String::new(),
            on: true,
            strength: 0.5,
            filters: vec![Filter::peak(100.0, 4.0, 1.0)],
        }),
        ..Profile::default()
    };
    let c = p.chain(FS);
    assert_eq!((c.left.len(), c.right.len()), (1, 2));
    assert_eq!(c.left[0].gain_db, 2.0);
    assert!(!c.is_symmetric());
}

#[test]
fn old_and_newer_profiles() {
    let old = r#"{"name":"My headphones, warm","device":"x","preamp_db":-3.0,"filters":[{"type":"low_shelf","freq_hz":105,"gain_db":3.0,"q":0.7,"on":true}]}"#;
    let p = Profile::from_json(old).unwrap();
    assert_eq!(p.preamp, Preamp { auto: false, db: -3.0 });
    assert_eq!(p.filters.len(), 1);
    assert!(Profile::from_json(r#"{"schema":99,"name":"x"}"#).is_err());
    assert!(Profile::from_json("[]").is_err());
}

#[test]
fn warnings_advise_without_changing_anything() {
    let mut p = Profile::default();
    assert!(warning::check(&p, FS).is_empty());
    p.filters.push(Filter::peak(40_000.0, 30.0, 50.0));
    p.preamp = Preamp { auto: false, db: 10.0 };
    let before = p.clone();
    let w = warning::check(&p, FS);
    assert!(w.len() >= 4);
    assert!(w.windows(2).all(|x| x[0].severity >= x[1].severity));
    assert_eq!(p, before);
}

#[test]
fn undo_merges_drags() {
    let mut h = History::default();
    let mut p = Profile::default();
    for i in 0..50 {
        h.record(&p, Some("drag 0"));
        p.preamp.db = f64::from(i);
    }
    h.seal();
    h.record(&p, None);
    p.name = "b".into();
    assert!(h.undo(&mut p) && p.preamp.db == 49.0 && p.name == "Untitled");
    assert!(h.undo(&mut p) && p.preamp.db == 0.0);
    assert!(!h.undo(&mut p));
    assert!(h.redo(&mut p) && h.redo(&mut p) && p.name == "b" && !h.can_redo());
}

#[test]
fn sliders_are_plain_bands() {
    let mut f = vec![Filter::peak(1000.0, 2.0, 3.0)];
    sliders::set(&mut f, 5, 4.0);
    assert_eq!(f.len(), 2);
    assert_eq!(sliders::position(&f, 5), 4.0);
    sliders::set(&mut f, 5, 0.0);
    sliders::tidy(&mut f);
    assert_eq!(f.len(), 1);
}

#[test]
fn numbers_people_type() {
    for (s, v) in [
        ("1.2k", 1200.0),
        ("1,2 kHz", 1200.0),
        ("1,200 Hz", 1200.0),
        ("\u{2212}3 dB", -3.0),
        ("+2,5", 2.5),
        ("0,125", 0.125),
        ("20", 20.0),
    ] {
        assert_eq!(text::parse_number(s), Some(v), "{s}");
    }
    for s in ["", "abc", "nan", "inf", "1e999", "--3"] {
        assert_eq!(text::parse_number(s), None, "{s}");
    }
    assert_eq!(text::freq(1234.0), "1.23 kHz");
    assert_eq!(text::freq(105.0), "105 Hz");
    assert_eq!(text::gain(-2.5), "\u{2212}2.5 dB");
    assert_eq!(text::gain(-0.01), "0 dB");
}

/// Net gain change at a frequency after following a request.
fn effect(request: &str, freq: f64) -> f64 {
    let s = rules::suggest(request, &[]).unwrap_or_else(|| panic!("not understood: {request}"));
    let mut p = Profile::default();
    change::apply_changes(&mut p, &s.changes).unwrap();
    curve::response_at(&p.filters, freq, FS)
}

#[test]
fn plain_english_rules() {
    for (request, freq, up) in [
        ("more bass", 50.0, true),
        ("less bass", 50.0, false),
        ("too much bass", 50.0, false),
        ("there's not enough bass", 50.0, true),
        ("bass", 50.0, true),
        ("I want much more sub bass", 35.0, true),
        ("it's too boomy", 120.0, false),
        ("sounds muddy", 280.0, false),
        ("less muddy", 280.0, false),
        ("boxy", 450.0, false),
        ("a bit nasal", 1000.0, false),
        ("less harsh", 3500.0, false),
        ("this is harsh and tiring", 3500.0, false),
        ("too sibilant", 7000.0, false),
        ("it sounds tinny", 5000.0, false),
        ("sounds thin", 100.0, true),
        ("a little dull", 10_000.0, true),
        ("too bright", 10_000.0, false),
        ("brighter", 10_000.0, true),
        ("warmer please", 200.0, true),
        ("vocals sound far away", 2000.0, true),
        ("louder vocals", 2200.0, true),
        ("clearer speech for podcasts", 2500.0, true),
        ("I can't hear footsteps in games", 3000.0, true),
        ("more punch", 90.0, true),
        ("more air and sparkle", 14_000.0, true),
        ("reduce the cymbals", 9000.0, false),
        ("more treble", 12_000.0, true),
        ("smoother", 3500.0, false),
        ("more drums", 90.0, true),
        ("night listening", 40.0, false),
        ("v-shaped", 50.0, true),
        ("boost mids", 1000.0, true),
        ("more guitars", 2800.0, true),
        ("the treble lacks air", 14_000.0, true),
    ] {
        let e = effect(request, freq);
        assert!(if up { e > 0.4 } else { e < -0.4 }, "{request}: {e} dB at {freq} Hz");
    }
    let both = "more bass, less harsh";
    assert!(effect(both, 50.0) > 1.0 && effect(both, 3500.0) < -1.0);
    let subject = "a bit more bass, and the vocals are harsh";
    assert!(effect(subject, 50.0) > 0.5 && effect(subject, 2200.0) < 0.0 && effect(subject, 3500.0) < -0.5);
    let run_on = "more bass less harsh";
    assert!(effect(run_on, 50.0) > 1.0 && effect(run_on, 3500.0) < -1.0);
    let opposite = "less bass but more vocals";
    assert!(effect(opposite, 40.0) < -1.0 && effect(opposite, 2200.0) > 1.0);
    assert!(effect("a bit more bass", 50.0) < effect("more bass", 50.0));
    assert!(effect("way more bass", 50.0) > effect("more bass", 50.0));
    assert!(rules::suggest("what is the weather", &[]).is_none());
    assert!(rules::suggest("", &[]).is_none());
}

#[test]
fn repeated_requests_grow_one_band() {
    let mut p = Profile::default();
    for _ in 0..3 {
        let s = rules::suggest("more bass", &p.filters).unwrap();
        change::apply_changes(&mut p, &s.changes).unwrap();
    }
    assert_eq!(p.filters.len(), 1);
    assert_eq!(p.filters[0].gain_db, 12.0);
    let s = rules::suggest("reset", &p.filters).unwrap();
    change::apply_changes(&mut p, &s.changes).unwrap();
    assert!(p.filters.is_empty());
}

#[test]
fn changes_apply_fully_or_not_at_all() {
    let mut p = Profile { filters: vec![Filter::peak(100.0, 1.0, 1.0); 3], ..Profile::default() };
    let before = p.clone();
    assert!(change::apply_changes(&mut p, &[change::Change::Remove(0), change::Change::Remove(9)]).is_err());
    assert!(change::apply_changes(&mut p, &[change::Change::Add(Filter::peak(f64::NAN, 1.0, 1.0))]).is_err());
    assert_eq!(p, before);
    change::apply_changes(
        &mut p,
        &[change::Change::Remove(0), change::Change::Update(2, Filter::peak(500.0, 2.0, 1.0))],
    )
    .unwrap();
    assert_eq!(p.filters.len(), 2);
    assert_eq!(p.filters[1].freq_hz, 500.0);
    assert_eq!(
        change::describe_change(&before.filters, &change::Change::Remove(0)),
        "Remove band 1 (peak at 100 Hz, +1 dB, Q 1)"
    );
}

/// Simulated listener with a known threshold per tone.
fn run_test(threshold: impl Fn(hearing::Ear, f64) -> f64) -> hearing::Test {
    let mut t = hearing::Test::new();
    let mut guard = 0;
    while let Some(tone) = t.tone() {
        t.answer(tone.db >= threshold(tone.ear, tone.freq));
        guard += 1;
        assert!(guard < 600, "test never ends");
    }
    assert!(t.is_done() && t.progress() == 1.0);
    t
}

#[test]
fn hearing_test_finds_thresholds_and_compensates() {
    let t = run_test(|ear, f| if ear == hearing::Ear::Right && f == 4000.0 { -50.0 } else { -80.0 });
    for th in t.thresholds() {
        let want = if th.freq == 4000.0 { -50.0 } else { -80.0 };
        assert!((th.left.unwrap() + 80.0).abs() <= 5.0);
        assert!((th.right.unwrap() - want).abs() <= 5.0);
    }
    let comp = hearing::compensation(&t.thresholds());
    let at4k = comp.iter().find(|c| c.0 == 4000.0).unwrap();
    assert!(at4k.2 - at4k.1 > 8.0, "{comp:?}");
    let points: Vec<_> = comp.iter().map(|c| (c.0, c.2)).collect();
    let fit = hearing::fit(&points, FS);
    for (f, g) in points {
        assert!((curve::response_at(&fit, f, FS) - g).abs() < 0.5);
    }
}

#[test]
fn hearing_test_handles_extremes() {
    run_test(|_, _| -200.0);
    let deaf = run_test(|_, _| 100.0);
    let comp = hearing::compensation(&deaf.thresholds());
    assert!(comp.iter().all(|c| c.1 == 0.0 && c.2 == 0.0));
    assert!(hearing::fit(&[(1000.0, 0.0)], FS).is_empty());
}

#[test]
fn tones_start_and_end_silent() {
    let tone = hearing::Tone { ear: hearing::Ear::Left, freq: 1000.0, db: -20.0 };
    let s = hearing::render_tone(tone, FS);
    assert!(s[0].abs() < 1e-6 && s.iter().skip(1).step_by(2).all(|r| *r == 0.0));
    let peak = s.iter().fold(0f32, |a, b| a.max(b.abs()));
    assert!((peak - 0.1).abs() < 0.002);
}

#[test]
fn peak_search_is_fast_enough_for_dragging() {
    let filters: Vec<Filter> = (0..40).map(|i| Filter::peak(30.0 * 1.18f64.powi(i), 5.0, 2.0)).collect();
    let t = std::time::Instant::now();
    for _ in 0..20 {
        std::hint::black_box(curve::peak_db(&filters, FS));
    }
    println!("40 bands: {:?} per peak search", t.elapsed() / 20);
}
