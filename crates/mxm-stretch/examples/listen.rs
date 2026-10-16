//! Stretches a recording to listen to: each ratio given, written beside each other as 32-bit float
//! WAV files named for it.
//!
//! ```text
//! cargo run -p mxm-stretch --example listen -- <recording> <folder> <ratio>...
//! ```
//!
//! A ratio is the new length over the old, so 1.25 is a quarter longer, played at 0.8 of the tempo.

use mxm_audio_file::{Target, write};
use mxm_audio_file_decode::{AtLimit, Keep, Limits, decode_file};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(recording), Some(folder)) = (args.next(), args.next()) else {
        eprintln!("usage: listen <recording> <folder> <ratio>...");
        std::process::exit(2);
    };
    let ratios: Vec<f64> = args
        .map(|ratio| ratio.parse().expect("a ratio is a number, such as 1.25"))
        .collect();
    let limits = Limits::new(48_000 * 60 * 20, AtLimit::Refuse, Keep::AllUpTo(2));
    let decoded = decode_file(&recording, &limits).expect("the recording can't be read");
    let channels: Vec<Vec<f32>> = (0..decoded.channels)
        .map(|c| {
            decoded
                .interleaved
                .iter()
                .skip(c)
                .step_by(decoded.channels)
                .copied()
                .collect()
        })
        .collect();
    let input: Vec<&[f32]> = channels.iter().map(Vec::as_slice).collect();
    let name = PathBuf::from(&recording)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "recording".into());
    for ratio in ratios {
        let frames = (decoded.frames() as f64 * ratio).round() as usize;
        let started = std::time::Instant::now();
        let out =
            mxm_stretch::stretch(&input, frames, decoded.sample_rate, &AtomicBool::new(false))
                .expect("the stretch failed");
        let took = started.elapsed();
        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|n| out.iter().map(move |channel| channel[n]))
            .collect();
        let path = PathBuf::from(&folder).join(format!("{name}-x{ratio}.wav"));
        write(
            &path,
            &interleaved,
            decoded.channels as u16,
            decoded.sample_rate,
            Target::WavFloat32,
        )
        .expect("the file can't be written");
        println!("{} ({took:.2?})", path.display());
    }
}
