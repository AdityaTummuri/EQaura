//! Turns a curve into the band gains of Android's DynamicsProcessing effect,
//! which works on the bins of a Fourier transform.

use eq_core::curve::Grid;
use eq_core::filter::Filter;

/// Neighbouring bands differ by at most this much, or by one bin.
const RATIO: f64 = 1.06;

/// The last bin of every band. Low bands hold a single bin each.
pub fn edges(block: usize) -> Vec<usize> {
    let top = (block / 2).max(1);
    let mut out = vec![1];
    while out[out.len() - 1] < top {
        let k = out[out.len() - 1];
        out.push(((k as f64 * RATIO).floor() as usize).max(k + 1).min(top));
    }
    out
}

pub fn cutoffs(block: usize, rate: f64) -> Vec<f64> {
    edges(block).iter().map(|k| *k as f64 * rate / block as f64).collect()
}

/// One gain per band: the curve averaged over the bins the band covers.
pub fn gains(filters: &[Filter], block: usize, rate: f64) -> Vec<f64> {
    let edges = edges(block);
    if filters.is_empty() {
        return vec![0.0; edges.len()];
    }
    let top = edges[edges.len() - 1];
    // The bin at 0 Hz is left to the first real one, and filters are not
    // evaluated exactly at half the sample rate, where some fall to nothing.
    let freqs = (1..=top).map(|k| (k as f64 * rate / block as f64).min(rate * 0.499)).collect();
    let curve = Grid::new(freqs, rate).response(filters);
    let mut start = 0;
    edges
        .iter()
        .map(|end| {
            let part = &curve[start..*end];
            start = *end;
            let mean = part.iter().sum::<f64>() / part.len() as f64;
            // Silence is deep enough at -120 dB; infinities are not allowed.
            (mean.clamp(-120.0, 120.0) * 100.0).round() / 100.0
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eq_core::curve::response_at;
    use eq_core::filter::Kind;

    #[test]
    fn bands_rise_and_reach_the_top() {
        for block in [1024, 2048, 4096] {
            let e = edges(block);
            assert!(e.windows(2).all(|w| w[0] < w[1]));
            assert_eq!(e[e.len() - 1], block / 2);
            assert!(e.len() <= 110, "{block}: {} bands", e.len());
            assert_eq!(cutoffs(block, 48_000.0).len(), gains(&[], block, 48_000.0).len());
        }
    }

    #[test]
    fn bands_follow_the_curve() {
        let filters = [
            Filter::new(Kind::LowShelf, 105.0, 5.0, 0.7),
            Filter::peak(1000.0, -4.0, 1.4),
            Filter::peak(3200.0, 6.0, 2.0),
            Filter::new(Kind::HighShelf, 9000.0, -3.0, 0.7),
        ];
        for (block, rate) in [(1024, 48_000.0), (2048, 44_100.0), (4096, 48_000.0)] {
            let e = edges(block);
            let g = gains(&filters, block, rate);
            let mut band = 0;
            for k in 1..block / 2 {
                if k > e[band] {
                    band += 1;
                }
                let f = k as f64 * rate / block as f64;
                if !(30.0..=16_000.0).contains(&f) {
                    continue;
                }
                let want = response_at(&filters, f, rate);
                assert!((g[band] - want).abs() < 0.5, "{block}: {f:.0} Hz {} {want}", g[band]);
            }
        }
    }

    #[test]
    fn extreme_filters_stay_finite() {
        let filters = [Filter::new(Kind::Notch, 1000.0, 0.0, 30.0), Filter::peak(50.0, 400.0, 0.01)];
        assert!(gains(&filters, 2048, 48_000.0).iter().all(|g| g.is_finite() && g.abs() <= 120.0));
    }
}
