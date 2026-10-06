//! Emits the figures behind `docs/mxm-measure.html` as JSON, measured by the crate itself.
//!
//! ```bash
//! cargo run -p mxm-measure --release --example showcase > docs/mxm-measure-data.json
//! ```
//!
//! **Every number on that page comes from here**, which is the point: a page explaining a
//! measurement library with hand-drawn curves would be exactly the thing this crate exists to stop.
//! JSON is written by hand because the crate has no dependencies and an example is not the place to
//! start acquiring them.

use mxm_measure::{convert, level, observe, pitch, spectrum, stimulus};

const RATE: f64 = 48_000.0;

fn main() {
    let mut out = String::from("{\n");
    out.push_str(&tuning_ruler());
    out.push_str(&aliasing_controls());
    out.push_str(&leakage());
    out.push_str(&transfer_gain());
    out.push_str(&a_weighting());
    out.push_str(&flatness());
    out.push_str(&quantisation());
    out.push_str(&worst_step());
    out.push_str(&endpoints());
    out.push_str("  \"rate\": 48000\n}\n");
    print!("{out}");
}

// --- helpers -------------------------------------------------------------------------------------

fn numbers(v: &[f64]) -> String {
    v.iter()
        .map(|x| {
            if x.is_finite() {
                format!("{x:.6}")
            } else {
                "null".to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Magnitude spectrum in dB relative to the loudest bin, decimated to `points`.
fn spectrum_db(x: &[f32], points: usize) -> Vec<f64> {
    let n = x.len().next_power_of_two();
    let mut re: Vec<f64> = x.iter().map(|&s| f64::from(s)).collect();
    re.resize(n, 0.0);
    let mut im = vec![0.0f64; n];
    spectrum::fft(&mut re, &mut im).expect("a power-of-two length");

    let half = n / 2;
    let mags: Vec<f64> = (0..half).map(|k| re[k].hypot(im[k])).collect();
    let loudest = mags.iter().copied().fold(0.0f64, f64::max).max(1e-30);

    // Decimate by taking the peak of each group, so a narrow line is not stepped over.
    let group = half.div_ceil(points);
    mags.chunks(group)
        .map(|c| {
            let peak = c.iter().copied().fold(0.0f64, f64::max);
            convert::amplitude_db(peak / loudest).max(-140.0)
        })
        .collect()
}

// --- 1. the ruler that could not resolve what it claimed ------------------------------------------

fn tuning_ruler() -> String {
    // Deliberately off-grid pitches: at an exact 55 Hz, two seconds holds exactly 110 cycles and the
    // counting method happens to be right. That is why the defect survived.
    let pitches = [55.3f64, 110.7, 221.3, 441.9, 883.1, 1766.5, 3533.7];
    let seconds = 2.0;
    let n = (seconds * RATE) as usize;

    let (mut counted, mut interpolated) = (Vec::new(), Vec::new());
    for hz in pitches {
        let x = stimulus::sine(n, hz, RATE, 0.8);

        // The retired ruler: count rising zero crossings, divide by the window.
        let mut crossings = 0usize;
        let mut previous = 0.0f32;
        for &s in &x {
            if previous <= 0.0 && s > 0.0 {
                crossings += 1;
            }
            previous = s;
        }
        counted.push(convert::cents_error(crossings as f64 / seconds, hz));

        let measured = pitch::frequency_by_crossings(&x, RATE).expect("a tone has a frequency");
        interpolated.push(convert::cents_error(measured, hz));
    }

    format!(
        "  \"tuning\": {{\"hz\": [{}], \"counted\": [{}], \"interpolated\": [{}]}},\n",
        numbers(&pitches),
        numbers(&counted),
        numbers(&interpolated)
    )
}

// --- 2. the two controls that keep the ruler honest -----------------------------------------------

fn aliasing_controls() -> String {
    let n = 1 << 13;
    let periods = 129;
    let clean = stimulus::additive_saw(n, periods);
    let dirty = stimulus::trivial_saw(n, periods);

    format!(
        "  \"controls\": {{\"clean\": [{}], \"dirty\": [{}], \"clean_db\": {:.2}, \"dirty_db\": {:.2}}},\n",
        numbers(&spectrum_db(&clean, 400)),
        numbers(&spectrum_db(&dirty, 400)),
        spectrum::alias_to_signal_db(&clean, periods).expect("a clean control reads"),
        spectrum::alias_to_signal_db(&dirty, periods).expect("a dirty control reads"),
    )
}

// --- 3. why the analysis needs no window function -------------------------------------------------

fn leakage() -> String {
    let n = 1 << 12;
    // Exactly periodic: a whole, odd number of cycles in the window.
    let on_grid = stimulus::periodic_frequency(n, 1_000.0, RATE);
    let exact = stimulus::periodic_sine(n, 1_000.0, RATE, 0.8);
    // Half a bin away, which is the worst case.
    let off_grid = on_grid + 0.5 * RATE / n as f64;
    let smeared = stimulus::sine(n, off_grid, RATE, 0.8);

    format!(
        "  \"leakage\": {{\"exact\": [{}], \"smeared\": [{}], \"on_grid_hz\": {on_grid:.3}, \"off_grid_hz\": {off_grid:.3}}},\n",
        numbers(&spectrum_db(&exact, 300)),
        numbers(&spectrum_db(&smeared, 300)),
    )
}

// --- 4. a system's response, measured against a closed form ---------------------------------------

fn transfer_gain() -> String {
    let cutoff = 1_000.0f64;
    let coefficient = 1.0 - (-std::f64::consts::TAU * cutoff / RATE).exp();

    let (mut hz, mut measured, mut ideal) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..=48 {
        // 20 Hz to 20 kHz, logarithmically.
        let f = 20.0 * (20_000.0f64 / 20.0).powf(f64::from(i) / 48.0);
        hz.push(f);

        let mut state = 0.0f64;
        let probe = spectrum::Probe::new(f, 1e-3, 0.5, 16.0, RATE);
        let gain = spectrum::transfer_gain(probe, RATE, |x| {
            state += coefficient * (f64::from(x) - state);
            state as f32
        })
        .expect("a probe at a real frequency has a gain");
        measured.push(convert::amplitude_db(gain));

        // The closed form for a one-pole, which the measurement is scored against.
        ideal.push(convert::amplitude_db(
            1.0 / (1.0 + (f / cutoff).powi(2)).sqrt(),
        ));
    }

    format!(
        "  \"transfer\": {{\"hz\": [{}], \"measured\": [{}], \"ideal\": [{}], \"cutoff\": {cutoff}}},\n",
        numbers(&hz),
        numbers(&measured),
        numbers(&ideal)
    )
}

// --- 5. the audibility weighting ------------------------------------------------------------------

fn a_weighting() -> String {
    let (mut hz, mut db) = (Vec::new(), Vec::new());
    for i in 0..=60 {
        let f = 10.0 * (20_000.0f64 / 10.0).powf(f64::from(i) / 60.0);
        hz.push(f);
        db.push(convert::amplitude_db(spectrum::a_weight(f)).max(-80.0));
    }
    format!(
        "  \"aweight\": {{\"hz\": [{}], \"db\": [{}]}},\n",
        numbers(&hz),
        numbers(&db)
    )
}

// --- 6. one number that separates a tone from noise -----------------------------------------------

fn flatness() -> String {
    let n = 1 << 13;
    let noise = stimulus::noise(n, 4, 0.5);
    let tone = stimulus::periodic_sine(n, 1_000.0, RATE, 0.5);
    let saw = stimulus::additive_saw(n, 129);

    let widen = |x: &[f32]| -> Vec<f64> { x.iter().map(|&s| f64::from(s)).collect() };
    let flat = |x: &[f32]| {
        spectrum::spectral_flatness_db(&widen(x), RATE, 100.0, 18_000.0).expect("a flatness")
    };

    format!(
        "  \"flatness\": {{\"noise\": {:.2}, \"tone\": {:.2}, \"saw\": {:.2}, \
         \"noise_spec\": [{}], \"tone_spec\": [{}], \"saw_spec\": [{}]}},\n",
        flat(&noise),
        flat(&tone),
        flat(&saw),
        numbers(&spectrum_db(&noise, 240)),
        numbers(&spectrum_db(&tone, 240)),
        numbers(&spectrum_db(&saw, 240)),
    )
}

// --- 7. the correction nine WAV writers took ------------------------------------------------------

fn quantisation() -> String {
    // Error in LSB for each method, across a sweep of the range.
    let (mut truncated, mut rounded) = (Vec::new(), Vec::new());
    let steps = 300;
    for i in 0..steps {
        let sample = (i as f32 / steps as f32) * 1.6 - 0.8;
        let exact = f64::from(sample) * 32_767.0;
        let old = (sample.clamp(-1.0, 1.0) * 32_767.0) as i16;
        // The encoder's published 16-bit rule (`mxm_audio_file::quantise`), stated inline so this
        // crate keeps no dependencies: clamp, scale by 32767, round to nearest.
        let new = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        truncated.push(f64::from(old) - exact);
        rounded.push(f64::from(new) - exact);
    }

    let worst = |v: &[f64]| v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;

    format!(
        "  \"quantise\": {{\"truncated\": [{}], \"rounded\": [{}], \
         \"truncated_worst\": {:.4}, \"rounded_worst\": {:.4}, \
         \"truncated_bias\": {:.4}, \"rounded_bias\": {:.4}}},\n",
        numbers(&truncated),
        numbers(&rounded),
        worst(&truncated),
        worst(&rounded),
        mean(&truncated),
        mean(&rounded),
    )
}

// --- 8. what a click is, measured ------------------------------------------------------------------

fn worst_step() -> String {
    let n = 480;
    let mut clean = stimulus::sine(n, 200.0, RATE, 0.7);
    let mut clicked = clean.clone();
    // A discontinuity: the phase jumps, which is exactly what an ungated waveform reset does.
    for (i, s) in clicked.iter_mut().enumerate().skip(n / 2) {
        *s = (0.7 * (std::f64::consts::TAU * 200.0 * (i as f64 + 90.0) / RATE).sin()) as f32;
    }
    clean.truncate(n);

    let (clean_at, clean_step) = observe::worst_step(&clean).expect("a step");
    let (click_at, click_step) = observe::worst_step(&clicked).expect("a step");

    format!(
        "  \"click\": {{\"clean\": [{}], \"clicked\": [{}], \
         \"clean_at\": {clean_at}, \"clean_step\": {clean_step:.5}, \
         \"click_at\": {click_at}, \"click_step\": {click_step:.5}}},\n",
        numbers(&clean.iter().map(|&s| f64::from(s)).collect::<Vec<_>>()),
        numbers(&clicked.iter().map(|&s| f64::from(s)).collect::<Vec<_>>()),
    )
}

// --- 9. the factor of two at the ends of the spectrum ---------------------------------------------

fn endpoints() -> String {
    let n = 4096;
    let dc = stimulus::dc(n, 0.5);
    let nyquist: Vec<f32> = (0..n)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    let tone = stimulus::periodic_sine(n, 1_000.0, RATE, 0.5);
    let tone_hz = stimulus::periodic_frequency(n, 1_000.0, RATE);

    let at = |x: &[f32], hz: f64| spectrum::component_amplitude(x, hz, RATE).expect("a component");
    // What the defect reported: the ordinary factor of two applied everywhere.
    let naive = |x: &[f32], hz: f64| {
        let w = std::f64::consts::TAU * hz / RATE;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (k, &s) in x.iter().enumerate() {
            let p = w * k as f64;
            re += f64::from(s) * p.cos();
            im -= f64::from(s) * p.sin();
        }
        2.0 * re.hypot(im) / x.len() as f64
    };

    format!(
        "  \"endpoints\": {{\"labels\": [\"DC\",\"1 kHz tone\",\"Nyquist\"], \
         \"truth\": [0.5,0.5,0.5], \"fixed\": [{:.4},{:.4},{:.4}], \"naive\": [{:.4},{:.4},{:.4}], \
         \"peak\": {:.4}, \"rms\": {:.4}}},\n",
        at(&dc, 0.0),
        at(&tone, tone_hz),
        at(&nyquist, RATE / 2.0),
        naive(&dc, 0.0),
        naive(&tone, tone_hz),
        naive(&nyquist, RATE / 2.0),
        level::peak(&tone).expect("a finite tone"),
        level::rms(&tone).expect("a non-empty tone"),
    )
}
