//! `mxm-fft` — the collection's one FFT (`plans/plan-mxm-fft.md`).
//!
//! `rustfft` (complex) and `realfft` (real-input) do the arithmetic; this crate owns the
//! conventions, so they exist once:
//!
//! - **Forward is unnormalised, with a negative exponent**: `X[k] = Σ x[n]·e^(−2πi·kn/N)`.
//! - **Inverse is scaled by 1/N**, so a forward then an inverse returns the input.
//! - **Interleaved is the layout** ([`Complex`]); [`ComplexFft::forward_split`] and
//!   [`ComplexFft::inverse_split`] adapt split real and imaginary slices.
//! - **A plan is made ahead of use** ([`Planner`]), and planning allocates.
//! - **Executing takes caller scratch and allocates nothing.** A buffer of the wrong length is an
//!   [`Error`], and the transform leaves every buffer untouched.
//! - **Subnormals are flushed by the hardware while a transform runs** ([`denormals`]), and the
//!   previous state is restored afterwards. A caller that stores spectra still flushes its own state.

use std::sync::Arc;

pub use rustfft::num_complex::Complex;

pub mod denormals;

/// The sample types the collection transforms: `f32` and `f64`.
pub trait Sample: rustfft::FftNum + sealed::Sealed {}
impl Sample for f32 {}
impl Sample for f64 {}

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// Which buffer had the wrong length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buffer {
    /// The signal or spectrum transformed in place, or a real transform's input.
    Data,
    /// The real part of a split layout.
    Real,
    /// The imaginary part of a split layout.
    Imaginary,
    /// A real transform's output.
    Output,
    /// The interleaved work buffer a split transform runs in.
    Work,
    /// Scratch shorter than [`ComplexFft::scratch_len`] or [`RealFft::scratch_len`].
    Scratch,
}

/// A buffer of the wrong length. Nothing was transformed and no buffer was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    pub buffer: Buffer,
    /// The length required: exact for every buffer but [`Buffer::Scratch`], which is a minimum.
    pub expected: usize,
    pub got: usize,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} buffer has length {}, needs {}{}",
            self.buffer,
            self.got,
            if self.buffer == Buffer::Scratch {
                "at least "
            } else {
                ""
            },
            self.expected
        )
    }
}

impl std::error::Error for Error {}

fn exact(buffer: Buffer, expected: usize, got: usize) -> Result<(), Error> {
    if expected == got {
        Ok(())
    } else {
        Err(Error {
            buffer,
            expected,
            got,
        })
    }
}

fn at_least(expected: usize, got: usize) -> Result<(), Error> {
    if got >= expected {
        Ok(())
    } else {
        Err(Error {
            buffer: Buffer::Scratch,
            expected,
            got,
        })
    }
}

/// Makes plans. **Planning allocates, and cannot fail cleanly**: neither `rustfft` nor `realfft`
/// reserves fallibly, so planning aborts on out-of-memory as any `Vec` does. Plan off the audio
/// thread, ahead of use, and keep the plan.
pub struct Planner<T: Sample> {
    complex: rustfft::FftPlanner<T>,
    real: realfft::RealFftPlanner<T>,
}

impl<T: Sample> Default for Planner<T> {
    fn default() -> Self {
        Self {
            complex: rustfft::FftPlanner::new(),
            real: realfft::RealFftPlanner::new(),
        }
    }
}

impl<T: Sample> Planner<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// A complex transform of `len` points, forward and inverse. Any length is allowed, 0 and 1
    /// included.
    pub fn complex(&mut self, len: usize) -> ComplexFft<T> {
        let forward = self.complex.plan_fft_forward(len);
        let inverse = self.complex.plan_fft_inverse(len);
        let scratch_len = forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len());
        ComplexFft {
            len,
            forward,
            inverse,
            scratch_len,
        }
    }

    /// A real-input transform of `len` samples, `len / 2 + 1` bins, forward and inverse.
    pub fn real(&mut self, len: usize) -> RealFft<T> {
        let forward = self.real.plan_fft_forward(len);
        let inverse = self.real.plan_fft_inverse(len);
        let scratch_len = forward.get_scratch_len().max(inverse.get_scratch_len());
        RealFft {
            len,
            forward,
            inverse,
            scratch_len,
        }
    }
}

/// A planned complex transform of one length. Cloning shares the plan.
#[derive(Clone)]
pub struct ComplexFft<T: Sample> {
    len: usize,
    forward: Arc<dyn rustfft::Fft<T>>,
    inverse: Arc<dyn rustfft::Fft<T>>,
    scratch_len: usize,
}

impl<T: Sample> ComplexFft<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The scratch either direction needs, at least.
    pub fn scratch_len(&self) -> usize {
        self.scratch_len
    }

    /// Scratch for this plan. Allocates: make it ahead of use.
    pub fn make_scratch(&self) -> Vec<Complex<T>> {
        vec![Complex::new(T::zero(), T::zero()); self.scratch_len]
    }

    /// The forward transform in place, unnormalised.
    pub fn forward(
        &self,
        buffer: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
    ) -> Result<(), Error> {
        exact(Buffer::Data, self.len, buffer.len())?;
        at_least(self.scratch_len, scratch.len())?;
        self.run(&self.forward, buffer, scratch);
        Ok(())
    }

    /// The inverse transform in place, scaled by 1/N.
    pub fn inverse(
        &self,
        buffer: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
    ) -> Result<(), Error> {
        exact(Buffer::Data, self.len, buffer.len())?;
        at_least(self.scratch_len, scratch.len())?;
        self.run(&self.inverse, buffer, scratch);
        scale(buffer, self.len);
        Ok(())
    }

    /// [`forward`](Self::forward) on split real and imaginary slices, through `work`, an
    /// interleaved buffer of the same length.
    pub fn forward_split(
        &self,
        re: &mut [T],
        im: &mut [T],
        work: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
    ) -> Result<(), Error> {
        self.split(re, im, work, scratch, false)
    }

    /// [`inverse`](Self::inverse) on split real and imaginary slices, scaled by 1/N.
    pub fn inverse_split(
        &self,
        re: &mut [T],
        im: &mut [T],
        work: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
    ) -> Result<(), Error> {
        self.split(re, im, work, scratch, true)
    }

    fn split(
        &self,
        re: &mut [T],
        im: &mut [T],
        work: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
        inverse: bool,
    ) -> Result<(), Error> {
        exact(Buffer::Real, self.len, re.len())?;
        exact(Buffer::Imaginary, self.len, im.len())?;
        exact(Buffer::Work, self.len, work.len())?;
        at_least(self.scratch_len, scratch.len())?;
        for ((value, re), im) in work.iter_mut().zip(re.iter()).zip(im.iter()) {
            *value = Complex::new(*re, *im);
        }
        if inverse {
            self.run(&self.inverse, work, scratch);
            scale(work, self.len);
        } else {
            self.run(&self.forward, work, scratch);
        }
        for ((value, re), im) in work.iter().zip(re.iter_mut()).zip(im.iter_mut()) {
            *re = value.re;
            *im = value.im;
        }
        Ok(())
    }

    fn run(
        &self,
        fft: &Arc<dyn rustfft::Fft<T>>,
        buffer: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
    ) {
        if self.len == 0 {
            return;
        }
        let _flush = denormals::Guard::new();
        fft.process_with_scratch(buffer, &mut scratch[..self.scratch_len]);
    }
}

/// A planned real-input transform of one length: `len` samples to `len / 2 + 1` bins. Cloning
/// shares the plan.
#[derive(Clone)]
pub struct RealFft<T: Sample> {
    len: usize,
    forward: Arc<dyn realfft::RealToComplex<T>>,
    inverse: Arc<dyn realfft::ComplexToReal<T>>,
    scratch_len: usize,
}

impl<T: Sample> RealFft<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The bins a spectrum holds: `len / 2 + 1`.
    pub fn spectrum_len(&self) -> usize {
        self.len / 2 + 1
    }

    /// The scratch either direction needs, at least.
    pub fn scratch_len(&self) -> usize {
        self.scratch_len
    }

    /// Scratch for this plan. Allocates: make it ahead of use.
    pub fn make_scratch(&self) -> Vec<Complex<T>> {
        vec![Complex::new(T::zero(), T::zero()); self.scratch_len]
    }

    /// The forward transform, unnormalised: `signal` (`len` samples, garbage afterwards: the
    /// transform works in it) into `spectrum` (`len / 2 + 1` bins).
    pub fn forward(
        &self,
        signal: &mut [T],
        spectrum: &mut [Complex<T>],
        scratch: &mut [Complex<T>],
    ) -> Result<(), Error> {
        exact(Buffer::Data, self.len, signal.len())?;
        exact(Buffer::Output, self.spectrum_len(), spectrum.len())?;
        at_least(self.scratch_len, scratch.len())?;
        if self.len == 0 {
            spectrum.fill(Complex::new(T::zero(), T::zero()));
            return Ok(());
        }
        let _flush = denormals::Guard::new();
        let scratch = &mut scratch[..self.forward.get_scratch_len()];
        // The lengths were checked above, so the only error left cannot arise.
        let _ = self.forward.process_with_scratch(signal, spectrum, scratch);
        Ok(())
    }

    /// The inverse transform, scaled by 1/N: `spectrum` (`len / 2 + 1` bins, garbage afterwards)
    /// into `signal` (`len` samples). The imaginary parts a real signal's spectrum cannot have, at
    /// DC and (for an even length) at Nyquist, are taken as zero.
    pub fn inverse(
        &self,
        spectrum: &mut [Complex<T>],
        signal: &mut [T],
        scratch: &mut [Complex<T>],
    ) -> Result<(), Error> {
        exact(Buffer::Data, self.spectrum_len(), spectrum.len())?;
        exact(Buffer::Output, self.len, signal.len())?;
        at_least(self.scratch_len, scratch.len())?;
        if self.len == 0 {
            return Ok(());
        }
        spectrum[0].im = T::zero();
        if self.len.is_multiple_of(2) {
            spectrum[self.len / 2].im = T::zero();
        }
        let _flush = denormals::Guard::new();
        let scratch = &mut scratch[..self.inverse.get_scratch_len()];
        // The lengths were checked and the imaginary parts zeroed, so no error can arise.
        let _ = self.inverse.process_with_scratch(spectrum, signal, scratch);
        let n = T::from_usize(self.len).unwrap_or_else(T::one);
        for value in signal.iter_mut() {
            *value = *value / n;
        }
        Ok(())
    }
}

fn scale<T: Sample>(buffer: &mut [Complex<T>], len: usize) {
    let n = T::from_usize(len).unwrap_or_else(T::one);
    for value in buffer.iter_mut() {
        *value = Complex::new(value.re / n, value.im / n);
    }
}
