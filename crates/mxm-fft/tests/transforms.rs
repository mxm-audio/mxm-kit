//! The conventions against closed forms, every length the collection uses and more, both
//! precisions, both layouts, and the rejection contract.

use mxm_fft::{Buffer, Complex, ComplexFft, Error, Planner, RealFft};
use std::f64::consts::TAU;

fn zero() -> Complex<f64> {
    Complex::new(0.0, 0.0)
}

fn forward(fft: &ComplexFft<f64>, signal: &[Complex<f64>]) -> Vec<Complex<f64>> {
    let mut buffer = signal.to_vec();
    let mut scratch = fft.make_scratch();
    fft.forward(&mut buffer, &mut scratch).unwrap();
    buffer
}

fn close(a: Complex<f64>, b: Complex<f64>, tolerance: f64) -> bool {
    (a - b).norm() <= tolerance
}

#[test]
fn forward_has_a_negative_exponent_and_no_normalisation() {
    let n = 64;
    let fft = Planner::<f64>::new().complex(n);
    // e^(+2πi·n/N) lands entirely in bin 1, at height N, only under a negative-exponent forward.
    let signal: Vec<_> = (0..n)
        .map(|i| Complex::from_polar(1.0, TAU * i as f64 / n as f64))
        .collect();
    let spectrum = forward(&fft, &signal);
    for (k, value) in spectrum.iter().enumerate() {
        let expected = if k == 1 { n as f64 } else { 0.0 };
        assert!(
            close(*value, Complex::new(expected, 0.0), 1e-9),
            "bin {k}: {value}"
        );
    }
}

#[test]
fn a_delta_is_a_flat_spectrum_and_a_shifted_delta_a_phase_ramp() {
    for n in [1, 2, 3, 8, 12, 97, 128] {
        let fft = Planner::<f64>::new().complex(n);
        let mut delta = vec![zero(); n];
        delta[0] = Complex::new(1.0, 0.0);
        assert!(
            forward(&fft, &delta)
                .iter()
                .all(|x| close(*x, Complex::new(1.0, 0.0), 1e-12))
        );
        if n > 1 {
            let mut shifted = vec![zero(); n];
            shifted[1] = Complex::new(1.0, 0.0);
            for (k, value) in forward(&fft, &shifted).iter().enumerate() {
                let expected = Complex::from_polar(1.0, -TAU * k as f64 / n as f64);
                assert!(close(*value, expected, 1e-12), "n {n}, bin {k}");
            }
        }
    }
}

#[test]
fn a_real_sinusoid_is_half_its_length_in_its_bin() {
    let n = 4_096;
    let fft: RealFft<f64> = Planner::new().real(n);
    let mut signal: Vec<f64> = (0..n)
        .map(|i| (TAU * 100.0 * i as f64 / n as f64).cos())
        .collect();
    let mut spectrum = vec![zero(); fft.spectrum_len()];
    let mut scratch = fft.make_scratch();
    fft.forward(&mut signal, &mut spectrum, &mut scratch)
        .unwrap();
    assert_eq!(spectrum.len(), n / 2 + 1);
    for (k, value) in spectrum.iter().enumerate() {
        let expected = if k == 100 { n as f64 / 2.0 } else { 0.0 };
        assert!(
            close(*value, Complex::new(expected, 0.0), 1e-8),
            "bin {k}: {value}"
        );
    }
}

#[test]
fn a_real_even_signal_has_a_real_spectrum() {
    let n = 256;
    let fft = Planner::<f64>::new().complex(n);
    let signal: Vec<_> = (0..n)
        .map(|i| {
            let j = i.min(n - i) as f64;
            Complex::new((-j * j / 200.0).exp(), 0.0)
        })
        .collect();
    assert!(forward(&fft, &signal).iter().all(|x| x.im.abs() < 1e-12));
}

fn noise(n: usize, seed: &mut u64) -> Vec<f64> {
    (0..n)
        .map(|_| {
            *seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (*seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        })
        .collect()
}

#[test]
fn every_length_round_trips_in_both_precisions_and_both_kinds() {
    let mut seed = 7;
    let mut planner64 = Planner::<f64>::new();
    let mut planner32 = Planner::<f32>::new();
    let lengths = [
        1,
        2,
        3,
        4,
        5,
        7,
        8,
        16,
        17,
        100,
        127,
        128,
        255,
        256,
        1_000,
        1_024,
        4_096,
        65_536,
        1 << 20,
    ];
    for n in lengths {
        let re = noise(n, &mut seed);
        let im = noise(n, &mut seed);

        let fft = planner64.complex(n);
        let original: Vec<_> = re
            .iter()
            .zip(&im)
            .map(|(&a, &b)| Complex::new(a, b))
            .collect();
        let mut buffer = original.clone();
        let mut scratch = fft.make_scratch();
        fft.forward(&mut buffer, &mut scratch).unwrap();
        let spectrum64 = buffer.clone();
        fft.inverse(&mut buffer, &mut scratch).unwrap();
        let error = buffer
            .iter()
            .zip(&original)
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, f64::max);
        assert!(error < 1e-12, "complex f64 at {n}: {error}");

        // f32 agrees with f64 within its own precision.
        let fft32 = planner32.complex(n);
        let mut buffer32: Vec<_> = original
            .iter()
            .map(|x| Complex::new(x.re as f32, x.im as f32))
            .collect();
        let mut scratch32 = fft32.make_scratch();
        fft32.forward(&mut buffer32, &mut scratch32).unwrap();
        let scale = (n as f64).sqrt();
        let disagreement = buffer32
            .iter()
            .zip(&spectrum64)
            .map(|(a, b)| (Complex::new(f64::from(a.re), f64::from(a.im)) - b).norm())
            .fold(0.0, f64::max);
        assert!(
            disagreement < 2e-5 * scale * (n as f64).log2().max(1.0),
            "f32 against f64 at {n}: {disagreement}"
        );

        let real = planner64.real(n);
        let mut signal = re.clone();
        let mut spectrum = vec![zero(); real.spectrum_len()];
        let mut scratch = real.make_scratch();
        real.forward(&mut signal, &mut spectrum, &mut scratch)
            .unwrap();
        // The real transform's bins are the complex transform's of the same real signal.
        let complex_of_real = forward(
            &fft,
            &re.iter().map(|&x| Complex::new(x, 0.0)).collect::<Vec<_>>(),
        );
        for (a, b) in spectrum.iter().zip(&complex_of_real) {
            assert!(close(*a, *b, 1e-9 * scale), "real against complex at {n}");
        }
        real.inverse(&mut spectrum, &mut signal, &mut scratch)
            .unwrap();
        let error = signal
            .iter()
            .zip(&re)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(error < 1e-12, "real f64 at {n}: {error}");
    }
}

#[test]
fn the_largest_length_the_collection_needs_round_trips() {
    // 2^22 points: the convolution's longest offline transform.
    let n = 1 << 22;
    let fft: RealFft<f32> = Planner::new().real(n);
    let original: Vec<f32> = noise(n, &mut 11).iter().map(|&x| x as f32).collect();
    let mut signal = original.clone();
    let mut spectrum = vec![Complex::new(0.0f32, 0.0); fft.spectrum_len()];
    let mut scratch = fft.make_scratch();
    fft.forward(&mut signal, &mut spectrum, &mut scratch)
        .unwrap();
    fft.inverse(&mut spectrum, &mut signal, &mut scratch)
        .unwrap();
    let error = signal
        .iter()
        .zip(&original)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(error < 1e-4, "{error}");
}

#[test]
fn the_imaginary_parts_a_real_spectrum_cannot_have_are_ignored() {
    let n = 16;
    let fft: RealFft<f64> = Planner::new().real(n);
    let mut spectrum = vec![zero(); fft.spectrum_len()];
    spectrum[0] = Complex::new(n as f64, 5.0);
    spectrum[n / 2] = Complex::new(0.0, -3.0);
    let mut signal = vec![0.0; n];
    let mut scratch = fft.make_scratch();
    fft.inverse(&mut spectrum, &mut signal, &mut scratch)
        .unwrap();
    assert!(
        signal.iter().all(|x| (x - 1.0).abs() < 1e-12),
        "DC alone, at 1"
    );
}

#[test]
fn the_split_layout_keeps_measures_contract() {
    let mut planner = Planner::<f64>::new();
    // n = 1 is allowed and is the identity.
    let one = planner.complex(1);
    let (mut re, mut im) = (vec![0.25], vec![-0.5]);
    let mut work = vec![zero(); 1];
    let mut scratch = one.make_scratch();
    one.forward_split(&mut re, &mut im, &mut work, &mut scratch)
        .unwrap();
    assert_eq!((re[0], im[0]), (0.25, -0.5));

    let n = 32;
    let fft = planner.complex(n);
    let mut seed = 3;
    let re0 = noise(n, &mut seed);
    let im0 = noise(n, &mut seed);
    let (mut re, mut im) = (re0.clone(), im0.clone());
    let mut work = vec![zero(); n];
    let mut scratch = fft.make_scratch();
    fft.forward_split(&mut re, &mut im, &mut work, &mut scratch)
        .unwrap();
    let interleaved = forward(
        &fft,
        &re0.iter()
            .zip(&im0)
            .map(|(&a, &b)| Complex::new(a, b))
            .collect::<Vec<_>>(),
    );
    for k in 0..n {
        assert!(close(Complex::new(re[k], im[k]), interleaved[k], 1e-12));
    }
    fft.inverse_split(&mut re, &mut im, &mut work, &mut scratch)
        .unwrap();
    for k in 0..n {
        assert!((re[k] - re0[k]).abs() < 1e-12 && (im[k] - im0[k]).abs() < 1e-12);
    }

    // A wrong length is refused, and neither slice is touched.
    let (mut short, mut im) = (re0[..n - 1].to_vec(), im0.clone());
    assert_eq!(
        fft.forward_split(&mut short, &mut im, &mut work, &mut scratch),
        Err(Error {
            buffer: Buffer::Real,
            expected: n,
            got: n - 1
        })
    );
    assert_eq!((short, im), (re0[..n - 1].to_vec(), im0.clone()));
    let (mut re, mut short) = (re0.clone(), im0[..n - 1].to_vec());
    assert_eq!(
        fft.forward_split(&mut re, &mut short, &mut work, &mut scratch)
            .map_err(|e| e.buffer),
        Err(Buffer::Imaginary)
    );
    assert_eq!((re, short), (re0.clone(), im0[..n - 1].to_vec()));
}

#[test]
fn a_wrong_length_transforms_nothing() {
    let mut planner = Planner::<f64>::new();
    let fft = planner.complex(64);
    let original = vec![Complex::new(1.0, 2.0); 63];
    let mut buffer = original.clone();
    let mut scratch = fft.make_scratch();
    assert_eq!(
        fft.forward(&mut buffer, &mut scratch).map_err(|e| e.buffer),
        Err(Buffer::Data)
    );
    assert_eq!(buffer, original);
    if fft.scratch_len() > 0 {
        let mut buffer = vec![Complex::new(1.0, 2.0); 64];
        let mut short = vec![zero(); fft.scratch_len() - 1];
        assert_eq!(
            fft.inverse(&mut buffer, &mut short).map_err(|e| e.buffer),
            Err(Buffer::Scratch)
        );
        assert_eq!(buffer, vec![Complex::new(1.0, 2.0); 64]);
    }

    let real = planner.real(64);
    let mut signal = vec![0.5; 64];
    let mut spectrum = vec![zero(); 32];
    let mut scratch = real.make_scratch();
    assert_eq!(
        real.forward(&mut signal, &mut spectrum, &mut scratch)
            .map_err(|e| e.buffer),
        Err(Buffer::Output)
    );
    assert_eq!((signal, spectrum), (vec![0.5; 64], vec![zero(); 32]));
    let mut spectrum = vec![Complex::new(0.5, 0.5); 33];
    let mut signal = vec![0.0; 65];
    assert_eq!(
        real.inverse(&mut spectrum, &mut signal, &mut scratch)
            .map_err(|e| e.buffer),
        Err(Buffer::Output)
    );
    assert_eq!(
        spectrum,
        vec![Complex::new(0.5, 0.5); 33],
        "untouched, imaginary parts too"
    );
}

#[test]
fn longer_scratch_is_accepted() {
    let fft = Planner::<f32>::new().complex(100);
    let mut buffer = vec![Complex::new(1.0f32, 0.0); 100];
    let mut scratch = vec![Complex::new(0.0f32, 0.0); fft.scratch_len() + 50];
    fft.forward(&mut buffer, &mut scratch).unwrap();
    assert!((buffer[0].re - 100.0).abs() < 1e-3);
}

#[test]
fn length_zero_plans_and_transforms_nothing_in_both_precisions_and_every_kind() {
    fn zero<T: mxm_fft::Sample>() {
        let mut planner = Planner::<T>::new();
        let complex = planner.complex(0);
        let mut scratch = complex.make_scratch();
        let mut buffer: Vec<Complex<T>> = Vec::new();
        assert!(complex.forward(&mut buffer, &mut scratch).is_ok());
        assert!(complex.inverse(&mut buffer, &mut scratch).is_ok());
        let (mut re, mut im, mut work) = (Vec::<T>::new(), Vec::<T>::new(), Vec::new());
        assert!(
            complex
                .forward_split(&mut re, &mut im, &mut work, &mut scratch)
                .is_ok()
        );
        assert!(
            complex
                .inverse_split(&mut re, &mut im, &mut work, &mut scratch)
                .is_ok()
        );

        let real = planner.real(0);
        assert_eq!(real.spectrum_len(), 1);
        let mut scratch = real.make_scratch();
        let mut signal: Vec<T> = Vec::new();
        let mut spectrum = vec![Complex::new(T::one(), T::one())];
        assert!(
            real.forward(&mut signal, &mut spectrum, &mut scratch)
                .is_ok()
        );
        assert_eq!(spectrum[0], Complex::new(T::zero(), T::zero()));
        assert!(
            real.inverse(&mut spectrum, &mut signal, &mut scratch)
                .is_ok()
        );
    }
    zero::<f32>();
    zero::<f64>();
}
