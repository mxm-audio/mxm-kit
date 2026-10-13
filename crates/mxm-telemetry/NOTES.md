# NOTES.md — crates/mxm-telemetry

The detail behind [AGENTS.md](AGENTS.md).

## The policy, and the five it replaced

The meter's read side was one body in every plugin (`take_peak` ×18, `clipped` ×20, `clear_clip`
×19, and `mxm-mono-01`'s `acknowledge_clip`). The write side had drifted into five policies for a
non-finite peak:

- twelve plugins combined with `f32::max` or an equivalent, so `+inf` became the level and latched
  the clip, and NaN was dropped without a trace;
- `mxm-mono-01` and the two drum plugins ignored a non-finite peak entirely, neither a level nor a
  clip;
- `mxm-fx-delay`, `mxm-fx-oldify`, `mxm-creative-sampler` and `mxm-fx-convolution` stored it as zero;
- `mxm-bucket-delay` and `mxm-fx-curve` could store NaN as the level: one combine compared
  `current >= NaN`, which is false, and the other clamped, which passes NaN, then compared bits.

None was reachable when they were replaced — every block peak was built with `f32::max` from DSP
output the numeric contracts bound — so no plugin's behaviour changed in practice. The policy
chosen is the architecture audit's (O10): **a broken render shows as clipped, never as a number.**

`mxm-fx-convolution` latched only above full scale; exactly full scale latches now, as everywhere
else. The request slots were copied into all eleven instruments, one meaning spelled three to six ways
per method; two plugins did not clamp a request below the sentinel, which a CC value never reaches.
