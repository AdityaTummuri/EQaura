//! Presets that come with every EQaura app.

use crate::filter::{Filter, Kind};
use crate::profile::Profile;

type Row = (Kind, f64, f64, f64);
use Kind::{HighPass as HP, HighShelf as HS, LowShelf as LS, Peaking as PK};

const LIST: &[(&str, &[Row])] = &[
    ("Flat", &[]),
    ("Bass lift", &[(LS, 105.0, 5.0, 0.7), (PK, 250.0, -1.5, 0.9)]),
    ("Deep sub", &[(LS, 45.0, 6.0, 0.8), (PK, 180.0, -1.5, 0.8)]),
    (
        "Warm",
        &[(LS, 120.0, 2.5, 0.7), (PK, 220.0, 1.5, 0.8), (PK, 3500.0, -1.5, 1.0), (HS, 8000.0, -2.0, 0.7)],
    ),
    ("Clear vocals", &[(PK, 250.0, -2.0, 0.9), (PK, 2200.0, 3.0, 0.9), (HS, 9000.0, 1.0, 0.7)]),
    ("Bright", &[(PK, 3000.0, 1.0, 0.8), (HS, 7000.0, 3.5, 0.7)]),
    ("Smooth", &[(PK, 3500.0, -2.5, 1.1), (PK, 7500.0, -2.0, 1.6), (HS, 10_000.0, -1.0, 0.7)]),
    ("Energetic", &[(LS, 100.0, 4.0, 0.7), (PK, 800.0, -1.5, 0.7), (HS, 8000.0, 3.0, 0.7)]),
    (
        "Spoken word",
        &[(HP, 80.0, 0.0, 0.71), (PK, 200.0, -3.0, 0.8), (PK, 2500.0, 3.5, 0.9), (PK, 7000.0, -1.5, 2.0)],
    ),
    ("Late night", &[(LS, 80.0, -4.0, 0.7), (PK, 2500.0, 1.5, 0.8), (HS, 9000.0, -2.0, 0.7)]),
    ("Low volume", &[(LS, 105.0, 5.0, 0.7), (HS, 9000.0, 3.0, 0.7)]),
    ("Game detail", &[(LS, 100.0, -3.0, 0.7), (PK, 3000.0, 4.0, 0.9), (PK, 6000.0, 2.0, 1.2)]),
];

pub fn builtin() -> Vec<Profile> {
    LIST.iter()
        .map(|(name, rows)| Profile {
            name: (*name).into(),
            filters: rows.iter().map(|(k, f, g, q)| Filter::new(*k, *f, *g, *q)).collect(),
            ..Profile::default()
        })
        .collect()
}
