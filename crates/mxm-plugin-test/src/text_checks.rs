//! Shared headless check that **every parameter's text survives the host's round trip**, and the
//! census that proves a refactor changed no text.
//!
//! A CLAP host formats a value, parses the text back to a value and formats that again, so a
//! reading that chooses its unit, its precision or its sign from the raw value can print one text,
//! parse to the other side of its own switch and print another (D6, `plans/plan-sibling-audit.md`).
//! `clap-validator` 0.4.1's `param-conversions` walks a fixed grid of `ceil(4000/N)` points per
//! parameter, so a clean validator run proves nothing about the sliver either side of a switch —
//! which is why this walk probes those slivers itself.
//!
//! It was a test copied into each plugin's `params.rs`: ten whole-parameter walks, only two small
//! groups of them alike, each probing what its author had last been bitten by, and several plugins
//! with none (`plans/plan-mxm-params.md` P2). This is their union, for every plugin.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::Params;
use nice_plug::util;

/// The environment variable naming a directory to write the census into. Unset, nothing is written.
pub const CENSUS: &str = "MXM_TEXT_CENSUS";

/// Plain values either side of which the collection's readings change unit, precision or words —
/// probed on every parameter, each with its two normalised neighbours. A value outside a
/// parameter's range clamps to an end, which the grids already visit, so a probe meant for another
/// parameter costs a duplicate and nothing else.
const COLLECTION_PROBES: &[f32] = &[
    // Seconds read `{:.0} ms` or `{:.1} ms` below one second and `{:.2} s` above: either side of
    // each millisecond rounding edge, one second, and the `1.00 s` bucket.
    0.9994, 0.999_49, 0.9995, 0.999_51, 0.9996, 0.999_94, 0.999_949, 0.999_95, 0.999_951, 0.999_96,
    0.9999, 1.0, 1.000_01, 1.004, 1.005, 1.006,
    // Hertz read tenths or whole hertz below a kilohertz and tenths of one above: either side of
    // the tenth that rounds to 999.5 Hz and of the hundredth that rounds to 999.95 Hz, of 1000 Hz,
    // and of 1050 Hz and 1050.5 Hz, where `1.0 kHz` becomes `1.1 kHz`.
    999.4, 999.44, 999.45, 999.46, 999.49, 999.5, 999.9, 999.94, 999.95, 999.96, 1_000.0, 1_000.04,
    1_000.1, 1_049.9, 1_050.0, 1_050.1, 1_050.4, 1_050.45, 1_050.5, 1_050.6,
];

/// Plain values whose normalised neighbourhoods are walked ulp by ulp, [`NEIGHBOURS`] each side: a
/// skewed range's inverse can land a hair off a round value, and the hair is where D6 lived.
const EDGES: &[f32] = &[0.0, 0.9995, 0.999_95, 1.0, 999.5, 999.95, 1_000.0];

/// How many normalised ulps either side of each edge are walked.
const NEIGHBOURS: usize = 64;

/// Typed inputs the census parses on every parameter, so the parse direction is recorded as well as
/// the formatted one: bare numbers, every unit spelling the collection's parsers accept, signs, and
/// text no parser should take.
const TYPED: &[&str] = &[
    "",
    "abc",
    "0",
    "-0",
    "1",
    "0.5",
    "50",
    "-50",
    "100",
    "1000",
    "50 %",
    "50%",
    "-50 %",
    "0 %",
    "-0 %",
    "250 ms",
    "250ms",
    "1 s",
    "1s",
    "1.5 s",
    "0.5 s",
    "440 Hz",
    "440hz",
    "1 kHz",
    "1.2 kHz",
    "1khz",
    "-12 dB",
    "+3 dB",
    "0 dB",
    "+12",
    "-12",
    "12 st",
    "-1 oct",
    "+50 cents",
];

/// Every parameter, with the unit on as the host sees it, is printed, parsed and printed again, and
/// must read the same text:
///
/// - **every parameter**: the collection's `i / 19` grid and `clap-validator` 0.4.1's own grid, in
///   CLAP's units (normalised × step count, as the wrapper hands them over), and every probe in
///   [`COLLECTION_PROBES`] and `boundaries` with its two normalised neighbours;
/// - **a continuous parameter** also: a 1/1000 grid, either side of zero at each decade, either
///   side of 0 dB for a gain, and [`NEIGHBOURS`] ulps either side of every edge in [`EDGES`] and
///   `boundaries` that lies inside its range.
///
/// `boundaries` is the plugin's own: plain values where one of its readings switches that the
/// collection's list does not name, in any parameter's units. A text that does not parse back is a
/// failure too.
///
/// With [`CENSUS`] naming a directory, the walk also writes `<directory>/<package>.txt`: see
/// [`census`] for what it holds. The directory is the caller's choice, never a default.
///
/// # Panics
///
/// With every text that changed, or did not parse, through the host's conversion.
pub fn every_text_round_trips(params: &impl Params, boundaries: &[f32]) {
    let map = params.param_map();
    let validator_points = 4_000_usize.div_ceil(map.len().max(1)).clamp(5, 100);
    let probes = continuous_probes();
    let mut failures = BTreeSet::new();
    let mut census = std::env::var_os(CENSUS).map(|_| String::new());
    for (id, ptr, _group) in &map {
        // SAFETY: `params` owns every parameter these pointers name and outlives the loop; this is
        // the access the wrapper makes through CLAP.
        unsafe {
            let steps = ptr.step_count();
            let scale = steps.unwrap_or(1) as f32;
            let mut values: Vec<f32> = (0..=19)
                .map(|i| i as f32 / 19.0)
                .chain((0..validator_points).map(|i| {
                    let clap = f64::from(scale) * (i as f64 / (validator_points - 1) as f64);
                    clap as f32 / scale
                }))
                .collect();
            for &plain in COLLECTION_PROBES.iter().chain(boundaries) {
                let at = ptr.preview_normalized(plain).clamp(0.0, 1.0);
                values.extend([at.next_down().max(0.0), at, at.next_up().min(1.0)]);
            }
            if steps.is_none() {
                values.extend((0..=1000).map(|i| i as f32 / 1000.0));
                values.extend(probes.iter().map(|&plain| ptr.preview_normalized(plain)));
                let (a, b) = (ptr.preview_plain(0.0), ptr.preview_plain(1.0));
                let (low, high) = (a.min(b), a.max(b));
                for &edge in EDGES.iter().chain(boundaries) {
                    if !(low <= edge && edge <= high) {
                        continue;
                    }
                    let mut up = ptr.preview_normalized(edge);
                    let mut down = up;
                    for _ in 0..=NEIGHBOURS {
                        values.extend([up, down]);
                        up = up.next_up().min(1.0);
                        down = down.next_down().max(0.0);
                    }
                }
            }
            let values: Vec<f32> = values
                .into_iter()
                .map(|value| value.clamp(0.0, 1.0))
                .collect();
            for &normalised in &values {
                if let Some(failure) = round_trip(id, *ptr, normalised) {
                    failures.insert(failure);
                }
            }
            if let Some(census) = census.as_mut() {
                census_lines(census, id, *ptr, values);
            }
        }
    }
    if let Some(census) = census {
        write_census(&census);
    }
    assert!(
        failures.is_empty(),
        "{} parameter texts changed through the host's conversion:\n{}",
        failures.len(),
        failures.into_iter().collect::<Vec<_>>().join("\n")
    );
}

/// The plain values a continuous parameter is probed at besides the grids: either side of zero at
/// each decade from 1e-7, where a plain `{:.N}` prints a negative zero, and either side of 0 dB for
/// a gain shown in decibels.
fn continuous_probes() -> Vec<f32> {
    let mut probes = vec![0.0];
    for decade in [1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1] {
        for multiple in [1.0, 4.0, 5.0, 6.0] {
            probes.extend([decade * multiple, -decade * multiple]);
        }
    }
    for db in [1e-4, 1e-3, 0.04, 0.05, 0.06] {
        probes.extend([util::db_to_gain(db), util::db_to_gain(-db)]);
    }
    probes
}

/// One trip through the host, as `vendor/nice-plug`'s CLAP wrapper makes it: the text carries the
/// unit, and the parsed text comes back through the parameter's normalised conversion — in CLAP's
/// units, normalised × step count — before it is formatted again. The failure, if any.
///
/// # Safety
///
/// `ptr` must point at a parameter that outlives the call.
unsafe fn round_trip(id: &str, ptr: ParamPtr, normalised: f32) -> Option<String> {
    unsafe {
        let scale = ptr.step_count().unwrap_or(1) as f32;
        let first = ptr.normalized_value_to_string(normalised, true);
        let Some(parsed) = ptr.string_to_normalized_value(&first) else {
            return Some(format!("{id}: {first:?} does not parse"));
        };
        let back = (f64::from(parsed) * f64::from(scale)) as f32 / scale;
        let second = ptr.normalized_value_to_string(back, true);
        (second != first).then(|| {
            format!(
                "{id}: {first:?} reads back as {second:?} (plain {})",
                ptr.preview_plain(normalised)
            )
        })
    }
}

/// **The census**: what each parameter prints across everything the walk visited, and what each of
/// [`TYPED`] parses to, so a refactor's before and after can be diffed (`plans/plan-mxm-params.md`
/// P3). Per parameter, the visited normalised values in order, collapsed to runs that print the
/// same text — `id`, the run's first and last value's bits, the text with its unit, and the bits
/// it parses back to — so every visited value's text is recorded without a line per value. Then
/// one line per typed input: `id`, the input, the bits it parses to or `-`.
///
/// **A refactor tool, not a golden.** Text through a skewed range goes through the platform's
/// `powf`, so a pinned census would be one platform's truth; nothing reads it but a diff.
///
/// # Safety
///
/// `ptr` must point at a parameter that outlives the call.
unsafe fn census_lines(out: &mut String, id: &str, ptr: ParamPtr, mut values: Vec<f32>) {
    values.sort_by(f32::total_cmp);
    values.dedup_by(|a, b| a.to_bits() == b.to_bits());
    let bits = |parsed: Option<f32>| {
        parsed.map_or_else(|| "-".to_owned(), |v| format!("{:08x}", v.to_bits()))
    };
    unsafe {
        let mut run: Option<(f32, f32, String)> = None;
        for value in values {
            let text = ptr.normalized_value_to_string(value, true);
            match &mut run {
                Some((_, last, current)) if *current == text => *last = value,
                _ => {
                    if let Some((first, last, text)) = run.take() {
                        let parsed = bits(ptr.string_to_normalized_value(&text));
                        let _ = writeln!(
                            out,
                            "{id}\t{:08x}\t{:08x}\t{text}\t{parsed}",
                            first.to_bits(),
                            last.to_bits()
                        );
                    }
                    run = Some((value, value, text));
                }
            }
        }
        if let Some((first, last, text)) = run {
            let parsed = bits(ptr.string_to_normalized_value(&text));
            let _ = writeln!(
                out,
                "{id}\t{:08x}\t{:08x}\t{text}\t{parsed}",
                first.to_bits(),
                last.to_bits()
            );
        }
        for input in TYPED {
            let parsed = bits(ptr.string_to_normalized_value(input));
            let _ = writeln!(out, "{id}\ttyped {input:?}\t{parsed}");
        }
    }
}

/// The census files this test process has begun: a second walk in one process — the sampler walks
/// once per source rate — appends to its file instead of replacing what the first one wrote.
static BEGUN: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// Writes the census as `<CENSUS>/<package>.txt`, the package named by the directory cargo ran the
/// test from — the plugin's own folder is its package's name throughout the collection. The first
/// walk in a process replaces the file; any later one appends.
fn write_census(census: &str) {
    let dir = PathBuf::from(std::env::var_os(CENSUS).expect("checked by the caller"));
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .expect("cargo sets CARGO_MANIFEST_DIR for every test it runs");
    let package = PathBuf::from(manifest)
        .file_name()
        .expect("a manifest directory has a name")
        .to_string_lossy()
        .into_owned();
    std::fs::create_dir_all(&dir).expect("the census directory can be created");
    let path = dir.join(format!("{package}.txt"));
    let mut begun = BEGUN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let first = begun.get_or_insert_with(HashSet::new).insert(path.clone());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(!first)
        .truncate(first)
        .open(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    file.write_all(census.as_bytes())
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
}
