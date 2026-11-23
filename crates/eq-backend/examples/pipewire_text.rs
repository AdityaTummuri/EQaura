//! Prints what EQaura gives PipeWire. Used by tools/pipewire_test.sh.
//! `config` prints the start settings, `live` the changes to the second sound.

use eq_backend::pipewire::{Routing, config, live_updates, slots, slots_for};
use eq_core::filter::{Channel, Filter, Kind};
use eq_core::profile::{Preamp, Profile};

fn main() {
    let manual = Preamp { auto: false, db: -12.0 };
    let first =
        Profile { filters: vec![Filter::peak(1000.0, 6.0, 1.0)], preamp: manual, ..Profile::default() };
    let second = Profile {
        filters: vec![
            Filter::new(Kind::HighShelf, 300.0, 9.0, 0.7),
            Filter { channel: Channel::Right, ..Filter::peak(1000.0, -8.0, 1.0) },
        ],
        preamp: manual,
        ..Profile::default()
    };
    let rate = 48_000.0;
    let (a, b) = (first.chain(rate), second.chain(rate));
    let n = slots_for(&a);
    let (from, to) = (slots(&a, rate, n), slots(&b, rate, n));
    match std::env::args().nth(1).as_deref() {
        Some("config") => {
            let routing = Routing { smart: false, target: Some("test-sink".into()) };
            print!("{}", config(&from, 48_000, &routing));
        }
        Some("live") => live_updates(&from, &to).iter().for_each(|m| println!("{m}")),
        Some("expect") => {
            let at =
                |f: &[Filter], c: &eq_core::Chain| eq_core::curve::response_at(f, 1000.0, rate) + c.preamp_db;
            println!("{:.3} {:.3}", at(&a.left, &a), at(&a.right, &a));
            println!("{:.3} {:.3}", at(&b.left, &b), at(&b.right, &b));
        }
        _ => eprintln!("config | live | expect"),
    }
}
