//! Loads a `.vst3` through clack-host, as MXM Player and newDAWn do, and reports what a host sees:
//! its plugins, one plugin's ports, latency and parameters, a sine run through it, and its state
//! saved and loaded back.
//!
//! ```text
//! cargo run -p mxm-vst3-host --example vst3_probe -- <path.vst3> [plugin id] [--set <param id>=<value>]...
//! ```

use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::latency::PluginLatency;
use clack_extensions::params::{ParamInfoBuffer, PluginParams};
use clack_extensions::state::PluginState;
use clack_extensions::tail::PluginTail;
use clack_host::events::Match;
use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent, ParamValueEvent};
use clack_host::prelude::*;
use clack_host::utils::Cookie;
use std::ffi::CString;

struct Probe;

impl HostHandlers for Probe {
    type Shared<'a> = ();
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 512;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A host serves probes first thing, so a module is read out of process.
    mxm_vst3_host::serve_probe();
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: vst3_probe <path.vst3> [plugin id]")?;
    let mut wanted_id = None;
    let mut settings: Vec<(u32, f64)> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--set" {
            let setting = args.next().ok_or("--set needs <param id>=<value>")?;
            let (id, value) = setting
                .split_once('=')
                .ok_or("--set needs <param id>=<value>")?;
            settings.push((id.parse()?, value.parse()?));
        } else {
            wanted_id = Some(arg);
        }
    }

    // SAFETY: probing a plugin means running its code.
    let classes = unsafe { mxm_vst3_host::probe(path.as_ref()) }?;
    println!("module: {} audio classes", classes.len());

    let entry =
        PluginEntry::load_from_clack::<mxm_vst3_host::Vst3Entry>(&CString::new(path.clone())?)?;
    let factory = entry.get_plugin_factory().ok_or("no plugin factory")?;
    let mut first_id = None;
    let mut instruments = Vec::new();
    for descriptor in factory.plugin_descriptors() {
        let id = descriptor
            .id()
            .map(|id| id.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = descriptor
            .name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let vendor = descriptor
            .vendor()
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default();
        let features: Vec<String> = descriptor
            .features()
            .map(|feature| feature.to_string_lossy().into_owned())
            .collect();
        println!(
            "plugin {id}  \"{name}\" by {vendor}  [{}]",
            features.join(", ")
        );
        if features.iter().any(|feature| feature == "instrument") {
            instruments.push(id.clone());
        }
        first_id.get_or_insert(id);
    }
    let id = wanted_id
        .or(first_id)
        .ok_or("the module offers no plugins")?;
    let is_instrument = instruments.contains(&id);

    let host_info = HostInfo::new("mxm-vst3-host probe", "mxm", "https://mxm.dk", "0")?;
    let mut instance = PluginInstance::<Probe>::new(
        |_| (),
        |_| (),
        &entry,
        &CString::new(id.clone())?,
        &host_info,
    )?;

    let ports: Option<PluginAudioPorts> = instance.plugin_shared_handle().get_extension();
    let mut channels = (0, 0);
    if let Some(ports) = ports {
        let mut handle = instance.plugin_handle();
        let mut buffer = AudioPortInfoBuffer::new();
        for is_input in [true, false] {
            for index in 0..ports.count(&mut handle, is_input) {
                if let Some(port) = ports.get(&mut handle, index, is_input, &mut buffer) {
                    println!(
                        "{} port {index}: \"{}\", {} channels",
                        if is_input { "input" } else { "output" },
                        String::from_utf8_lossy(port.name),
                        port.channel_count
                    );
                    if index == 0 {
                        if is_input {
                            channels.0 = port.channel_count as usize;
                        } else {
                            channels.1 = port.channel_count as usize;
                        }
                    }
                }
            }
        }
    }

    let params: Option<PluginParams> = instance.plugin_shared_handle().get_extension();
    let read_params = |instance: &mut PluginInstance<Probe>| -> Vec<(u32, String, f64, String)> {
        let Some(params) = params else {
            return Vec::new();
        };
        let mut handle = instance.plugin_handle();
        let mut info_buffer = ParamInfoBuffer::new();
        let mut text_buffer = [0u8; 256];
        let mut found = Vec::new();
        for index in 0..params.count(&mut handle) {
            let Some(info) = params.get_info(&mut handle, index, &mut info_buffer) else {
                continue;
            };
            let (id, name) = (info.id, String::from_utf8_lossy(info.name).into_owned());
            let value = params.get_value(&mut handle, id).unwrap_or(f64::NAN);
            let text = params
                .value_to_text(&mut handle, id, value, &mut text_buffer)
                .map(|text| String::from_utf8_lossy(text).into_owned())
                .unwrap_or_default();
            found.push((id.get(), name, value, text));
        }
        found
    };
    let before = read_params(&mut instance);
    println!("{} parameters:", before.len());
    for (id, name, value, text) in &before {
        println!("  {id:>10}  {name:<28} {value:>10.4}  {text}");
    }

    if let Some(latency) = instance
        .plugin_shared_handle()
        .get_extension::<PluginLatency>()
    {
        println!(
            "latency: {} frames",
            latency.get(&mut instance.plugin_handle())
        );
    }

    // A second of a 1 kHz sine at -12 dBFS, then a second of silence.
    let frames = SAMPLE_RATE as usize * 2;
    let amplitude = 10f32.powf(-12.0 / 20.0);
    let source: Vec<f32> = (0..frames)
        .map(|i| {
            if i < frames / 2 {
                amplitude * (std::f32::consts::TAU * 1000.0 * i as f32 / SAMPLE_RATE as f32).sin()
            } else {
                0.0
            }
        })
        .collect();

    let processor = instance.activate(
        |_, _| (),
        PluginAudioConfiguration {
            sample_rate: SAMPLE_RATE,
            min_frames_count: 1,
            max_frames_count: BLOCK as u32,
        },
    )?;
    let mut processor = processor.start_processing()?;
    let (in_channels, out_channels) = (channels.0.max(1), channels.1.max(1));
    let mut input_block = vec![vec![0.0f32; BLOCK]; in_channels];
    let mut output_block = vec![vec![0.0f32; BLOCK]; out_channels];
    let mut output = vec![Vec::with_capacity(frames); out_channels];
    let mut input_ports = AudioPorts::with_capacity(in_channels, 1);
    let mut output_ports = AudioPorts::with_capacity(out_channels, 1);
    let mut input_events = EventBuffer::new();
    let mut output_events = EventBuffer::new();
    let mut frame = 0;
    while frame < frames {
        let n = BLOCK.min(frames - frame);
        for channel in &mut input_block {
            channel[..n].copy_from_slice(&source[frame..frame + n]);
        }
        input_events.clear();
        output_events.clear();
        // An instrument plays middle C for the first second.
        let middle_c = Pckn::new(0u16, 0u16, 60u16, Match::All);
        if is_instrument && frame == 0 {
            input_events.push(&NoteOnEvent::new(0, middle_c, 0.8));
        }
        if is_instrument && frame < frames / 2 && frame + n >= frames / 2 {
            input_events.push(&NoteOffEvent::new(
                (frames / 2 - frame) as u32,
                middle_c,
                0.0,
            ));
        }
        if frame == 0 {
            for &(id, value) in &settings {
                if let Some(id) = ClapId::from_raw(id) {
                    input_events.push(&ParamValueEvent::new(
                        0,
                        id,
                        Pckn::match_all(),
                        value,
                        Cookie::empty(),
                    ));
                }
            }
        }
        let inputs = input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                input_block
                    .iter_mut()
                    .map(|b| InputChannel::from_buffer(&mut b[..n], false)),
            ),
        }]);
        let mut outputs = output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                output_block.iter_mut().map(|b| &mut b[..n]),
            ),
        }]);
        processor.process(
            &inputs,
            &mut outputs,
            &InputEvents::from_buffer(&input_events),
            &mut OutputEvents::from_buffer(&mut output_events),
            Some(frame as u64),
            None,
        )?;
        for (channel, block) in output.iter_mut().zip(&output_block) {
            channel.extend_from_slice(&block[..n]);
        }
        frame += n;
    }
    if let Some(tail) = instance
        .plugin_shared_handle()
        .get_extension::<PluginTail>()
    {
        println!("tail: {:?}", tail.get(&processor.plugin_handle()));
    }
    let processor = processor.stop_processing();
    instance.deactivate(processor);

    let rms = |samples: &[f32]| {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    };
    let db = |level: f32| 20.0 * level.max(1e-12).log10();
    println!(
        "in:  sine at {:.1} dBFS RMS",
        db(rms(&source[..frames / 2]))
    );
    for (index, channel) in output.iter().enumerate() {
        let peak = channel.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        println!(
            "out {index}: {:.1} dBFS RMS while the sine plays, {:.1} dBFS RMS in the silence after, peak {:.1} dBFS, finite {}",
            db(rms(&channel[frames / 4..frames / 2])),
            db(rms(&channel[frames * 3 / 4..])),
            db(peak),
            channel.iter().all(|s| s.is_finite())
        );
    }

    if let Some(state) = instance
        .plugin_shared_handle()
        .get_extension::<PluginState>()
    {
        let mut saved = Vec::new();
        state.save(&mut instance.plugin_handle(), &mut saved)?;
        state.load(
            &mut instance.plugin_handle(),
            &mut std::io::Cursor::new(&saved),
        )?;
        let after = read_params(&mut instance);
        let changed = before
            .iter()
            .zip(&after)
            .filter(|(a, b)| a.2 != b.2 && !(a.2.is_nan() && b.2.is_nan()))
            .count();
        println!(
            "state: {} bytes; after saving and loading it, {changed} parameters differ from before the run",
            saved.len()
        );
    }
    Ok(())
}
