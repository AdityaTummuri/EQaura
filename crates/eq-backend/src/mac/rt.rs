//! Code that runs on the audio thread: no locks, no allocation, no logging.

use eq_core::dsp::Stereo;
use objc2_core_audio_types::{AudioBuffer, AudioBufferList};
use rtrb::{Consumer, Producer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Largest block handled at once; longer cycles are split.
const BLOCK: usize = 1024;
/// Length of the crossfade when settings change.
const FADE: usize = 1024;

#[derive(Debug, Default)]
pub struct Shared {
    /// A sample that is not silence has arrived.
    pub heard: AtomicBool,
    pub cycles: AtomicU64,
}

pub struct Rt {
    pub current: Box<Stereo>,
    pub next: Option<Box<Stereo>>,
    pub fade: usize,
    pub incoming: Consumer<Box<Stereo>>,
    /// Old settings go back to the engine thread to be freed there.
    pub retired: Producer<Box<Stereo>>,
    /// False while only listening for the first sound.
    pub audible: bool,
    pub shared: std::sync::Arc<Shared>,
    l: [f64; BLOCK],
    r: [f64; BLOCK],
    l2: [f64; BLOCK],
    r2: [f64; BLOCK],
}

impl Rt {
    pub fn new(
        current: Box<Stereo>,
        incoming: Consumer<Box<Stereo>>,
        retired: Producer<Box<Stereo>>,
        audible: bool,
        shared: std::sync::Arc<Shared>,
    ) -> Box<Self> {
        Box::new(Self {
            current,
            next: None,
            fade: 0,
            incoming,
            retired,
            audible,
            shared,
            l: [0.0; BLOCK],
            r: [0.0; BLOCK],
            l2: [0.0; BLOCK],
            r2: [0.0; BLOCK],
        })
    }

    fn accept_new_settings(&mut self) {
        if self.next.is_none()
            && !self.retired.is_full()
            && let Ok(mut s) = self.incoming.pop()
        {
            s.inherit(&self.current);
            self.next = Some(s);
            self.fade = 0;
        }
    }

    fn process(&mut self, n: usize) {
        if let Some(next) = &mut self.next {
            self.l2[..n].copy_from_slice(&self.l[..n]);
            self.r2[..n].copy_from_slice(&self.r[..n]);
            next.left.process(&mut self.l2[..n]);
            next.right.process(&mut self.r2[..n]);
        }
        self.current.left.process(&mut self.l[..n]);
        self.current.right.process(&mut self.r[..n]);
        if self.next.is_some() {
            for i in 0..n {
                let t = ((self.fade + i) as f64 / FADE as f64).min(1.0);
                self.l[i] += (self.l2[i] - self.l[i]) * t;
                self.r[i] += (self.r2[i] - self.r[i]) * t;
            }
            self.fade += n;
            if self.fade >= FADE
                && let Some(next) = self.next.take()
            {
                let old = std::mem::replace(&mut self.current, next);
                // `accept_new_settings` checked that there is room.
                let _ = self.retired.push(old);
            }
        }
    }

    /// One audio cycle.
    ///
    /// # Safety
    /// Both lists must be valid for the duration of the call, as Core Audio
    /// guarantees inside an IO callback.
    pub unsafe fn cycle(&mut self, input: *const AudioBufferList, output: *mut AudioBufferList) {
        self.shared.cycles.fetch_add(1, Ordering::Relaxed);
        self.accept_new_settings();
        // SAFETY: per the function contract.
        let (ins, outs) = unsafe { (buffers(input), buffers_mut(output)) };
        for b in outs.iter() {
            if !b.mData.is_null() {
                // SAFETY: Core Audio owns mDataByteSize bytes at mData.
                unsafe { std::ptr::write_bytes(b.mData.cast::<u8>(), 0, b.mDataByteSize as usize) };
            }
        }
        // The tap is the last input stream; earlier ones belong to the device
        // itself, for example a headset microphone.
        let Some(tap) = ins.iter().rev().find(|b| !b.mData.is_null() && b.mNumberChannels > 0) else {
            return;
        };
        let in_ch = tap.mNumberChannels as usize;
        let frames = tap.mDataByteSize as usize / (4 * in_ch);
        // SAFETY: float samples, size taken from the buffer itself.
        let src = unsafe { std::slice::from_raw_parts(tap.mData.cast::<f32>(), frames * in_ch) };
        if !self.shared.heard.load(Ordering::Relaxed) && src.iter().any(|s| *s != 0.0) {
            self.shared.heard.store(true, Ordering::Relaxed);
        }
        if !self.audible {
            return;
        }
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK);
            for i in 0..n {
                let f = &src[(done + i) * in_ch..(done + i + 1) * in_ch];
                self.l[i] = f64::from(f[0]);
                self.r[i] = f64::from(if in_ch > 1 { f[1] } else { f[0] });
            }
            self.process(n);
            // SAFETY: each output buffer is checked against its own size.
            unsafe { self.write(outs, done, n) };
            done += n;
        }
    }

    unsafe fn write(&self, outs: &mut [AudioBuffer], at: usize, n: usize) {
        let total: u32 = outs.iter().map(|b| b.mNumberChannels).sum();
        let mut channel = 0;
        for b in outs.iter_mut() {
            let ch = b.mNumberChannels as usize;
            if b.mData.is_null() || ch == 0 {
                continue;
            }
            let frames = b.mDataByteSize as usize / (4 * ch);
            let n = n.min(frames.saturating_sub(at));
            // SAFETY: float samples, size taken from the buffer itself.
            let dst = unsafe { std::slice::from_raw_parts_mut(b.mData.cast::<f32>(), frames * ch) };
            for i in 0..n {
                let frame = &mut dst[(at + i) * ch..(at + i + 1) * ch];
                for (k, out) in frame.iter_mut().enumerate() {
                    let v = match channel + k {
                        // A mono device gets both ears.
                        0 if total == 1 => (self.l[i] + self.r[i]) * 0.5,
                        0 => self.l[i],
                        1 => self.r[i],
                        _ => break,
                    };
                    *out = v as f32;
                }
            }
            channel += ch;
            if channel >= 2 {
                break;
            }
        }
    }
}

unsafe fn buffers<'a>(list: *const AudioBufferList) -> &'a [AudioBuffer] {
    if list.is_null() {
        return &[];
    }
    // SAFETY: mBuffers is a C array with mNumberBuffers elements.
    unsafe {
        let n = (*list).mNumberBuffers as usize;
        std::slice::from_raw_parts((&raw const (*list).mBuffers).cast::<AudioBuffer>(), n)
    }
}

unsafe fn buffers_mut<'a>(list: *mut AudioBufferList) -> &'a mut [AudioBuffer] {
    if list.is_null() {
        return &mut [];
    }
    // SAFETY: mBuffers is a C array with mNumberBuffers elements.
    unsafe {
        let n = (*list).mNumberBuffers as usize;
        std::slice::from_raw_parts_mut((&raw mut (*list).mBuffers).cast::<AudioBuffer>(), n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eq_core::filter::Filter;
    use eq_core::profile::Profile;
    use std::sync::Arc;

    /// A fake AudioBufferList with room for several buffers. No audio device
    /// is involved in these tests.
    #[repr(C)]
    struct List<const N: usize> {
        count: u32,
        buffers: [AudioBuffer; N],
    }

    fn list<const N: usize>(data: &mut [(u32, Vec<f32>); N]) -> List<N> {
        let mut it = data.iter_mut();
        let buffers = std::array::from_fn(|_| {
            let (ch, v) = it.next().expect("N buffers");
            AudioBuffer {
                mNumberChannels: *ch,
                mDataByteSize: (v.len() * 4) as u32,
                mData: v.as_mut_ptr().cast(),
            }
        });
        List { count: N as u32, buffers }
    }

    fn run<const I: usize, const O: usize>(
        rt: &mut Rt,
        input: &mut [(u32, Vec<f32>); I],
        output: &mut [(u32, Vec<f32>); O],
    ) {
        let i = list(input);
        let mut o = list(output);
        // SAFETY: both lists point at live vectors of the stated sizes.
        unsafe { rt.cycle((&raw const i).cast(), (&raw mut o).cast()) };
    }

    type Ends = (Producer<Box<Stereo>>, Consumer<Box<Stereo>>);

    fn rt(p: &Profile, audible: bool) -> (Box<Rt>, Ends) {
        let (tx, rx) = rtrb::RingBuffer::new(4);
        let (back, retired) = rtrb::RingBuffer::new(8);
        let s = Box::new(Stereo::new(&p.chain(48_000.0), 48_000.0));
        (Rt::new(s, rx, back, audible, Arc::default()), (tx, retired))
    }

    fn sine(n: usize, freq: f64, offset: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v =
                    (0.2 * (2.0 * std::f64::consts::PI * freq * (i + offset) as f64 / 48_000.0).sin()) as f32;
                [v, v]
            })
            .collect()
    }

    fn boosted() -> Profile {
        Profile { filters: vec![Filter::peak(1000.0, 6.0, 1.0)], ..Profile::default() }
    }

    #[test]
    fn filters_the_tap_and_ignores_the_microphone() {
        let (mut rt, _ends) = rt(&boosted(), true);
        let mut peak = 0f32;
        for k in 0..40 {
            let mut input = [(1, vec![0.9f32; 3000]), (2, sine(3000, 100.0, k * 3000))];
            let mut output = [(2, vec![7.0f32; 6000])];
            run(&mut rt, &mut input, &mut output);
            if k > 10 {
                peak = output[0].1.iter().fold(peak, |a, b| a.max(b.abs()));
            }
        }
        // -6 dB preamp, almost no boost at 100 Hz
        assert!((peak - 0.1).abs() < 0.005, "{peak}");
        assert!(rt.shared.heard.load(Ordering::Relaxed));
    }

    #[test]
    fn stays_silent_while_only_listening() {
        let (mut rt, _ends) = rt(&boosted(), false);
        let mut input = [(2, vec![0.0f32; 1024])];
        let mut output = [(2, vec![7.0f32; 1024])];
        run(&mut rt, &mut input, &mut output);
        assert!(output[0].1.iter().all(|s| *s == 0.0));
        assert!(!rt.shared.heard.load(Ordering::Relaxed));
        let mut input = [(2, sine(512, 440.0, 0))];
        run(&mut rt, &mut input, &mut output);
        assert!(output[0].1.iter().all(|s| *s == 0.0));
        assert!(rt.shared.heard.load(Ordering::Relaxed));
    }

    #[test]
    fn settings_change_without_clicks() {
        let (mut rt, (mut tx, mut retired)) = rt(&Profile::default(), true);
        let mut last = 0f32;
        let mut jump = 0f32;
        for k in 0..200 {
            if k % 10 == 5 {
                let mut p = boosted();
                p.filters[0].gain_db = k as f64 / 10.0 - 10.0;
                let _ = tx.push(Box::new(Stereo::new(&p.chain(48_000.0), 48_000.0)));
            }
            let mut input = [(2, sine(512, 200.0, k * 512))];
            let mut output = [(2, vec![0f32; 1024])];
            run(&mut rt, &mut input, &mut output);
            for f in output[0].1.chunks(2) {
                jump = jump.max((f[0] - last).abs());
                last = f[0];
            }
            while retired.pop().is_ok() {}
        }
        // A 200 Hz sine at 0.2 moves at most 0.0053 per sample.
        assert!(jump < 0.0065, "largest step {jump}");
    }

    #[test]
    fn handles_unusual_layouts() {
        let (mut rt, _ends) = rt(&Profile::default(), true);
        // Two mono outputs, then eight channels, then mono, then short output.
        let mut input = [(2, [0.5f32, -0.25].repeat(256))];
        let mut out = [(1, vec![9f32; 256]), (1, vec![9f32; 256])];
        run(&mut rt, &mut input, &mut out);
        assert!(out[0].1.iter().all(|s| *s == 0.5) && out[1].1.iter().all(|s| *s == -0.25));
        let mut out = [(8, vec![9f32; 2048])];
        run(&mut rt, &mut input, &mut out);
        assert!(out[0].1.chunks(8).all(|f| f[0] == 0.5 && f[1] == -0.25 && f[2..].iter().all(|s| *s == 0.0)));
        let mut out = [(1, vec![9f32; 256])];
        run(&mut rt, &mut input, &mut out);
        assert!(out[0].1.iter().all(|s| *s == 0.125));
        let mut out = [(2, vec![9f32; 100])];
        run(&mut rt, &mut input, &mut out);
        let mut mono_in = [(1, vec![0.5f32; 64])];
        let mut out = [(2, vec![9f32; 128])];
        run(&mut rt, &mut mono_in, &mut out);
        assert!(out[0].1.iter().all(|s| *s == 0.5));
        let mut none: [(u32, Vec<f32>); 0] = [];
        run(&mut rt, &mut none, &mut out);
        assert!(out[0].1.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn real_time_budget() {
        let filters: Vec<Filter> = (0..40).map(|i| Filter::peak(40.0 * 1.15f64.powi(i), 2.0, 1.0)).collect();
        let (mut rt, _ends) = rt(&Profile { filters, ..Profile::default() }, true);
        let mut input = [(2, sine(512, 300.0, 0))];
        let mut output = [(2, vec![0f32; 1024])];
        let t = std::time::Instant::now();
        for _ in 0..200 {
            run(&mut rt, &mut input, &mut output);
        }
        println!("40 stereo bands, 512 frames: {:?} of 10.7 ms", t.elapsed() / 200);
    }
}
