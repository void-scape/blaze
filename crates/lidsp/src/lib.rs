#![no_std]

/// Mix a variable number of sample buffers into `output`.
#[macro_export]
macro_rules! mix {
    ($output:expr, $($samples:expr),+) => {
        let output_len = $output.len();
        $(debug_assert_eq!($samples.len(), output_len);)*
        for i in 0..output_len {
            let mut sum = 0f32;
            $(sum += $samples[i];)+
            $output[i] = sum.clamp(-1.0, 1.0);
        }
    }
}

/// Linearly scale `samples` by `factor`.
pub fn linear_volume(samples: &mut [f32], factor: f32) {
    for sample in samples.iter_mut() {
        *sample = (*sample * factor).clamp(-1.0, 1.0);
    }
}

/// Linearly scale `samples` by `factor` without clamping samples.
pub fn linear_volume_unclamped(samples: &mut [f32], factor: f32) {
    for sample in samples.iter_mut() {
        *sample *= factor;
    }
}

/// Fill `samples` with a sine wave at `frequency`.
pub fn sine(
    samples: &mut [f32],
    sample_rate: f32,
    channels: usize,
    phase: &mut f32,
    frequency: f32,
) {
    debug_assert!(samples.len().is_multiple_of(channels));
    use core::f32::consts::TAU;
    for frame in samples.chunks_mut(channels) {
        phase_inc(sample_rate, phase, frequency);
        let s = libm::sinf(*phase * TAU);
        frame.fill(s);
    }
}

/// Fill `samples` with a triangle wave at `frequency`.
pub fn triangle(
    samples: &mut [f32],
    sample_rate: f32,
    channels: usize,
    phase: &mut f32,
    frequency: f32,
) {
    debug_assert!(samples.len().is_multiple_of(channels));
    for frame in samples.chunks_mut(channels) {
        phase_inc(sample_rate, phase, frequency);
        let s = if *phase < 0.5 {
            4.0 * *phase - 1.0
        } else {
            3.0 - 4.0 * *phase
        };
        frame.fill(s);
    }
}

/// Fill `samples` with a square wave at `frequency` with `duty_cycle`.
pub fn square(
    samples: &mut [f32],
    sample_rate: f32,
    channels: usize,
    phase: &mut f32,
    frequency: f32,
    duty_cycle: f32,
) {
    debug_assert!(samples.len().is_multiple_of(channels));
    for frame in samples.chunks_mut(channels) {
        phase_inc(sample_rate, phase, frequency);
        let s = if *phase < duty_cycle { 1.0 } else { 0.0 };
        frame.fill(s);
    }
}

fn phase_inc(sample_rate: f32, phase: &mut f32, frequency: f32) {
    *phase += frequency / sample_rate;
    if *phase >= 1.0 {
        *phase -= 1.0;
    }
}

/// Statically allocated freeverb buffer.
///
/// Call [`freeverb`] will a sample buffer.
pub struct Freeverb {
    pub roomsize: f32,
    pub width: f32,
    pub damp: f32,
    pub wet: f32,
    pub dry: f32,
    //
    combs: [f32; 22_232],
    combl_data: [(usize, core::ops::Range<usize>, f32); 8],
    combr_data: [(usize, core::ops::Range<usize>, f32); 8],
    allpasses: [f32; 3218],
    allpassl_data: [(usize, core::ops::Range<usize>); 4],
    allpassr_data: [(usize, core::ops::Range<usize>); 4],
}

impl Default for Freeverb {
    fn default() -> Self {
        Self::new(0.5, 1.0, 0.5, 1.0 / 3.0, 0.0)
    }
}

impl Freeverb {
    pub fn new(roomsize: f32, width: f32, damp: f32, wet: f32, dry: f32) -> Self {
        Self {
            roomsize,
            width,
            damp,
            wet,
            dry,
            combs: [0.0; 22_232],
            combl_data: [
                (0, 0..1116, 0.0),
                (0, 2255..3443, 0.0),
                (0, 4658..5931, 0.0),
                (0, 7231..8587, 0.0),
                (0, 9966..11388, 0.0),
                (0, 12833..14324, 0.0),
                (0, 15838..17395, 0.0),
                (0, 18975..20592, 0.0),
            ],
            combr_data: [
                (0, 1116..2255, 0.0),
                (0, 3443..4658, 0.0),
                (0, 5931..7231, 0.0),
                (0, 8587..9966, 0.0),
                (0, 11388..12833, 0.0),
                (0, 14324..15838, 0.0),
                (0, 17395..18975, 0.0),
                (0, 20592..22232, 0.0),
            ],
            allpasses: [0.0; 3218],
            allpassl_data: [
                (0, 0..556),
                (0, 1135..1576),
                (0, 2040..2381),
                (0, 2745..2970),
            ],
            allpassr_data: [
                (0, 556..1135),
                (0, 1576..2040),
                (0, 2381..2745),
                (0, 2970..3218),
            ],
        }
    }
}

// Implementation based on these resources:
// - https://ccrma.stanford.edu/~jos/pasp/Freeverb.html
// - https://github.com/sinshu/freeverb
pub fn freeverb(freeverb: &mut Freeverb, samples: &mut [f32], channels: usize) {
    debug_assert!(samples.len().is_multiple_of(channels));

    let wet1 = freeverb.wet * (freeverb.width / 2.0 + 0.5);
    let wet2 = freeverb.wet * ((1.0 - freeverb.width) / 2.0);

    for frame in samples.chunks_mut(channels) {
        let mut outl = 0.0;
        let mut outr = 0.0;

        let inputl = frame[0];
        let inputr = if channels >= 2 { frame[1] } else { inputl };
        let input = (inputl + inputr) * 0.015;

        for (delay_index, range, filter_store) in freeverb.combl_data.iter_mut() {
            outl += lbcf(
                &mut freeverb.combs[range.clone()],
                delay_index,
                filter_store,
                freeverb.damp,
                1.0 - freeverb.damp,
                freeverb.roomsize,
                input,
            );
        }
        for (delay_index, range, filter_store) in freeverb.combr_data.iter_mut() {
            outr += lbcf(
                &mut freeverb.combs[range.clone()],
                delay_index,
                filter_store,
                freeverb.damp,
                1.0 - freeverb.damp,
                freeverb.roomsize,
                input,
            );
        }

        for (delay_index, range) in freeverb.allpassl_data.iter_mut() {
            outl = allpass(
                &mut freeverb.allpasses[range.clone()],
                delay_index,
                0.5,
                outl,
            );
        }
        for (delay_index, range) in freeverb.allpassr_data.iter_mut() {
            outr = allpass(
                &mut freeverb.allpasses[range.clone()],
                delay_index,
                0.5,
                outr,
            );
        }

        for (i, sample) in frame.iter_mut().enumerate() {
            if i == 0 {
                *sample = outl * wet1 + outr * wet2 + inputl * freeverb.dry;
            } else {
                *sample = outr * wet1 + outl * wet2 + inputr * freeverb.dry;
            }
        }
    }
}

/// Lowpass-Feedback Comb Filter.
fn lbcf(
    delay_buffer: &mut [f32],
    delay_index: &mut usize,
    filter_store: &mut f32,
    damp1: f32,
    damp2: f32,
    feedback: f32,
    sample: f32,
) -> f32 {
    let mut output = delay_buffer[*delay_index];
    undenormalize(&mut output);

    *filter_store = output * damp1 + *filter_store * damp2;
    undenormalize(filter_store);

    delay_buffer[*delay_index] = sample + *filter_store * feedback;
    *delay_index += 1;
    if *delay_index >= delay_buffer.len() {
        *delay_index = 0;
    }

    output
}

fn allpass(delay_buffer: &mut [f32], delay_index: &mut usize, feedback: f32, sample: f32) -> f32 {
    let mut bufout = delay_buffer[*delay_index];
    undenormalize(&mut bufout);

    let output = -sample + bufout;
    delay_buffer[*delay_index] = sample + bufout * feedback;
    *delay_index += 1;
    if *delay_index >= delay_buffer.len() {
        *delay_index = 0;
    }

    output
}

fn undenormalize(s: &mut f32) {
    if (s.to_bits() & 0x7f800000) == 0 {
        *s = 0.0;
    }
}
