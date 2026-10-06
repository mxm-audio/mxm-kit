# mxm-kit

The libraries every [MXM](https://mxm.dk) instrument and effect is built on, published under the
MIT licence so anyone can build CLAP instruments that look, feel and work the same — keyboard-first,
resizable, one design system — and that work fully in newDAWn.

| Crate | What it is |
|---|---|
| [`mxm-ui`](crates/ui) | The interface foundation: theme, typography, controls, paging, the layout tree and keyboard navigation |
| [`mxm-preset`](crates/mxm-preset) | Preset format, library, browser and the parameter binding |
| [`mxm-modulation`](crates/mxm-modulation) | Modulation routing: source frames, compaction, combination laws |
| [`mxm-modulation-params`](crates/mxm-modulation-params) | Its parameter and interface half |
| [`mxm-tempo`](crates/mxm-tempo) | Tempo sync: one ladder of musical divisions |
| [`mxm-keys`](crates/mxm-keys) | The keyboard language: keys in, gestures out, and the keymap |
| [`mxm-part-routing`](crates/mxm-part-routing) | Drum part routing, note arbitration and click-free destinations |
| [`mxm-control-map`](crates/mxm-control-map) | The controller-map standard and its schema |
| [`mxm-audio-file`](crates/mxm-audio-file) | WAV, AIFF and FLAC writing |
| [`mxm-audio-file-decode`](crates/mxm-audio-file-decode) | Audio reading through symphonia (MPL-2.0) |
| [`mxm-measure`](crates/mxm-measure) | The measurement rulers every DSP test uses |
| [`mxm-plugin-test`](crates/mxm-plugin-test) | The headless checks every plugin's tests share |
| [`mxm-xtask`](crates/mxm-xtask) | Bundling and control-map staging for a repository's `cargo xtask` |

`docs/` holds the normative [design system](docs/MXM_DESIGN_SYSTEM.md) and
[control map](docs/MXM_CONTROL_MAP.md), and the filter, oscillator and modulation theory behind the
DSP.

## Using it

Take the crates as git dependencies at a tag, and patch nice-plug with the fork every MXM plugin
builds with:

```toml
[dependencies]
mxm-ui = { git = "https://github.com/mxm-audio/mxm-kit", tag = "v0.4.0" }
mxm-preset = { git = "https://github.com/mxm-audio/mxm-kit", tag = "v0.4.0" }

[patch.crates-io]
nice-plug = { git = "https://github.com/mxm-audio/nice-plug", tag = "0.4.2-mxm.1" }
egui-baseview = { git = "https://github.com/mxm-audio/egui-baseview", tag = "0.7.2-mxm.1" }
```

Any MXM instrument repository under [github.com/mxm-audio](https://github.com/mxm-audio) is a
worked example.

## Building

Rust 1.95 or newer. On Linux, install ALSA, X11, xkbcommon and a GL loader first.

```bash
cargo test --workspace
```

## Licence

MIT — see [`LICENSE`](LICENSE). `mxm-audio-file-decode` links symphonia (MPL-2.0, notice compiled
in); `mxm-ui` embeds the Inter fonts (SIL Open Font License). The MXM name is not covered by the
licence; the products' `TRADEMARKS.md` says how it may be used.

Contributions are welcome; see [`CONTRIBUTING.md`](CONTRIBUTING.md). How the repository is organised,
and the rules each crate keeps, are in [`AGENTS.md`](AGENTS.md).
