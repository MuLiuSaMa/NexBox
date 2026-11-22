//! FxSound-compatible Dattorro plate reverb (LexReverb).
//!
//! Based on Jon Dattorro's AES paper (JAES Vol.45 No.9, 1997)
//! and FxSound's LexReverb implementation.
//!
//! Topology:
//!   Input(sum L+R) -> PreDelay -> BW_LP -> LAT1..4(allpass diffusers)
//!     -> diffuser_out
//!     -> Tank1: LAT5(decay diffuser) -> D1 taps -> Damping -> *decay -> LAT6 -> Taps/D2 -> D2_fb
//!     -> Tank2: LAT7(decay diffuser) -> D3 taps -> Damping -> *decay -> LAT8 -> Taps/D4 -> D4_fb
//!     -> Output matrix -> Wet/Dry mix

use std::f64::consts::PI;

// ── FxSound LexReverb constants (all times in seconds) ─────────────────

const LAT1_DELAY: f64 = 4.77e-3;
const LAT2_DELAY: f64 = 3.595e-3;
const LAT3_DELAY: f64 = 12.73e-3;
const LAT4_DELAY: f64 = 9.31e-3;

const LAT5_NOMINAL: f64 = 22.6e-3;
const LAT5_MOD_MAX_MS: f64 = 2.0e-3; // max modulation range

const D1_TAP1_DELAY: f64 = 10.1e-3;
const D1_TAP2_DELAY: f64 = 66.9e-3;
const D1_TAP3_DELAY: f64 = 121.9e-3;
const D1_TAP4_DELAY: f64 = 149.6e-3;

const LAT6_TAP1_DELAY: f64 = 6.28e-3;
const LAT6_TAP2_DELAY: f64 = 41.26e-3;
const LAT6_DELAY: f64 = 60.5e-3;

const D2_TAP1_DELAY: f64 = 35.8e-3;
const D2_TAP2_DELAY: f64 = 89.8e-3;
const D2_TAP3_DELAY: f64 = 125.0e-3;

const LAT7_NOMINAL: f64 = 30.5e-3;

const D3_TAP1_DELAY: f64 = 10.1e-3;
const D3_TAP2_DELAY: f64 = 70.9e-3;
const D3_TAP3_DELAY: f64 = 99.9e-3;
const D3_TAP4_DELAY: f64 = 141.7e-3;

const LAT8_TAP1_DELAY: f64 = 11.25e-3;
const LAT8_TAP2_DELAY: f64 = 64.3e-3;
const LAT8_DELAY: f64 = 89.2e-3;

const D4_TAP1_DELAY: f64 = 4.065e-3;
const D4_TAP2_DELAY: f64 = 67.1e-3;
const D4_TAP3_DELAY: f64 = 106.3e-3;

// Fixed lattice coefficients
const LAT12_COEFF: f64 = 0.75;
const LAT34_COEFF: f64 = 0.625;
const LAT57_COEFF: f64 = 0.70;

// 1-pole LP filter constants
const BANDWIDTH: f64 = 0.35011;
const ONE_MINUS_BW: f64 = 1.0 - BANDWIDTH;
const DAMPING: f64 = 0.40829;
const ONE_MINUS_DAMP: f64 = 1.0 - DAMPING;

// LFO modulation
const OSC_TABLE_SIZE: f64 = 8192.0;
const MOD_FREQ_STEP: f64 = 0.110871f64; // step per sample (table index units)
const MOD_DEPTH_SAMPLES: f64 = 27.7795; // modulation depth in samples

// Output scaling (internal, before wet/dry mix)
const OUTPUT_SCALE: f64 = 0.3;

// ── DattorroReverb ──────────────────────────────────────────────────────

pub struct DattorroReverb {
    // Input allpass diffusers (implicit delay from buffer size)
    lat1_buf: Vec<f64>,
    lat1_pos: usize,
    lat2_buf: Vec<f64>,
    lat2_pos: usize,
    lat3_buf: Vec<f64>,
    lat3_pos: usize,
    lat4_buf: Vec<f64>,
    lat4_pos: usize,

    // Pre-delay buffer (1 sample default)
    pre_buf: Vec<f64>,
    pre_pos: usize,

    // Tank 1: LAT5 (modulated decay diffuser) + D1 taps
    tank1_buf: Vec<f64>,
    tank1_pos: usize,
    // Tank 1b: LAT6 (standard lattice, explicit delay) + taps + D2 taps
    tank1b_buf: Vec<f64>,
    tank1b_pos: usize,

    // Tank 2: LAT7 (modulated decay diffuser) + D3 taps
    tank2_buf: Vec<f64>,
    tank2_pos: usize,
    // Tank 2b: LAT8 (standard lattice, explicit delay) + taps + D4 taps
    tank2b_buf: Vec<f64>,
    tank2b_pos: usize,

    // Tap offsets in samples
    d1_t1: usize, d1_t2: usize, d1_t3: usize, d1_t4: usize,
    l6_dly: usize, l6_t1: usize, l6_t2: usize,
    d2_t1: usize, d2_t2: usize, d2_t3: usize,
    d3_t1: usize, d3_t2: usize, d3_t3: usize, d3_t4: usize,
    l8_dly: usize, l8_t1: usize, l8_t2: usize,
    d4_t1: usize, d4_t2: usize, d4_t3: usize,

    // Modulated delay lengths (nominal, in samples)
    lat5_nom: f64,
    lat7_nom: f64,

    // 1-pole LP filter states
    bw_old: f64,
    damp1_old: f64,
    damp2_old: f64,

    // LFO state
    osc_phase: f64,
    osc_step: f64, // radians per sample
    mod_depth: f64, // modulation depth in samples

    // Cross-feedback value (D4 output from previous iteration)
    d4_fb: f64,
}

impl DattorroReverb {
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate;

        // Convert time constants to samples
        let to_samps = |t: f64| -> usize { (t * sr).round() as usize };

        let lat1_len = to_samps(LAT1_DELAY);
        let lat2_len = to_samps(LAT2_DELAY);
        let lat3_len = to_samps(LAT3_DELAY);
        let lat4_len = to_samps(LAT4_DELAY);

        let d1_t1 = to_samps(D1_TAP1_DELAY);
        let d1_t2 = to_samps(D1_TAP2_DELAY);
        let d1_t3 = to_samps(D1_TAP3_DELAY);
        let d1_t4 = to_samps(D1_TAP4_DELAY);

        let l6_t1 = to_samps(LAT6_TAP1_DELAY);
        let l6_t2 = to_samps(LAT6_TAP2_DELAY);
        let l6_delay = to_samps(LAT6_DELAY);

        let d2_t1 = to_samps(D2_TAP1_DELAY);
        let d2_t2 = to_samps(D2_TAP2_DELAY);
        let d2_t3 = to_samps(D2_TAP3_DELAY);

        let d3_t1 = to_samps(D3_TAP1_DELAY);
        let d3_t2 = to_samps(D3_TAP2_DELAY);
        let d3_t3 = to_samps(D3_TAP3_DELAY);
        let d3_t4 = to_samps(D3_TAP4_DELAY);

        let l8_t1 = to_samps(LAT8_TAP1_DELAY);
        let l8_t2 = to_samps(LAT8_TAP2_DELAY);
        let l8_delay = to_samps(LAT8_DELAY);

        let d4_t1 = to_samps(D4_TAP1_DELAY);
        let d4_t2 = to_samps(D4_TAP2_DELAY);
        let d4_t3 = to_samps(D4_TAP3_DELAY);

        let lat5_nom = LAT5_NOMINAL * sr;
        let lat7_nom = LAT7_NOMINAL * sr;
        let lat5_mod_max = LAT5_MOD_MAX_MS * sr;

        // Tank buffer sizes: max(allpass max delay, max tap offset)
        let tank1_max: f64 = (lat5_nom + lat5_mod_max).max(d1_t4 as f64);
        let tank1_size = (tank1_max.ceil() as usize).next_power_of_two();

        let tank1b_size = l6_delay.max(d2_t3).next_power_of_two();

        let tank2_max: f64 = (lat7_nom + lat5_mod_max).max(d3_t4 as f64);
        let tank2_size = (tank2_max.ceil() as usize).next_power_of_two();

        let tank2b_size = l8_delay
            .max(d4_t3)
            .next_power_of_two();

        // Pre-delay buffer: 1 sample (FxFxSound default)
        let pre_size = 2usize; // minimum for 1-sample pre-delay

        // Oscillator step: convert table-index step to radians
        let osc_step = MOD_FREQ_STEP * 2.0 * PI / OSC_TABLE_SIZE;
        // Scale modulation depth with sample rate to maintain ~0.58ms depth
        let mod_depth = MOD_DEPTH_SAMPLES * 48000.0 / sr;

        Self {
            lat1_buf: vec![0.0; lat1_len.max(1)],
            lat1_pos: 0,
            lat2_buf: vec![0.0; lat2_len.max(1)],
            lat2_pos: 0,
            lat3_buf: vec![0.0; lat3_len.max(1)],
            lat3_pos: 0,
            lat4_buf: vec![0.0; lat4_len.max(1)],
            lat4_pos: 0,
            pre_buf: vec![0.0; pre_size],
            pre_pos: 0,
            tank1_buf: vec![0.0; tank1_size],
            tank1_pos: 0,
            tank1b_buf: vec![0.0; tank1b_size],
            tank1b_pos: 0,
            tank2_buf: vec![0.0; tank2_size],
            tank2_pos: 0,
            tank2b_buf: vec![0.0; tank2b_size],
            tank2b_pos: 0,

            d1_t1, d1_t2, d1_t3, d1_t4,
            l6_dly: l6_delay, l6_t1, l6_t2,
            d2_t1, d2_t2, d2_t3,
            d3_t1, d3_t2, d3_t3, d3_t4,
            l8_dly: l8_delay, l8_t1, l8_t2,
            d4_t1, d4_t2, d4_t3,

            lat5_nom,
            lat7_nom,

            bw_old: 0.0,
            damp1_old: 0.0,
            damp2_old: 0.0,

            osc_phase: 0.0,
            osc_step,
            mod_depth,
            d4_fb: 0.0,
        }
    }

    /// Process a stereo sample pair through the reverb.
    /// Input: mono sum of L+R, Output: (reverb_L, reverb_R)
    pub fn process(&mut self, mono_in: f64, amount: f64) -> (f64, f64) {
        // FxSound bypass: MIDI <= 12 (~9.4% slider) treated as off
        if amount < 0.094 {
            return (0.0, 0.0);
        }

        // -- FxSound parameter mapping (amount 0..1 -> decay, lat6_coeff) --

        // Exponential decay curve: 0.095 (min) to 0.95 (max, FxSound)
        // Capped at 0.50 for feedback stability — prevents echo runaway
        let decay_raw = 0.095f64 * (0.95f64 / 0.095f64).powf(amount);
        let decay = decay_raw.min(0.50);
        let lat6_coeff = (decay + 0.15).clamp(0.25, 0.5);

        // -- Pre-delay (1 sample) --
        let pre_out = self.pre_buf[self.pre_pos];
        self.pre_buf[self.pre_pos] = mono_in;
        self.pre_pos = (self.pre_pos + 1) % self.pre_buf.len();

        // -- Bandwidth (1-pole lowpass) --
        let bw_out = pre_out * ONE_MINUS_BW + self.bw_old * BANDWIDTH;
        self.bw_old = bw_out;

        // -- Input diffusers (LAT1-4) --
        let lat1_out = Self::lattice_allpass(
            &mut self.lat1_buf, &mut self.lat1_pos, bw_out, LAT12_COEFF);
        let lat2_out = Self::lattice_allpass(
            &mut self.lat2_buf, &mut self.lat2_pos, lat1_out, LAT12_COEFF);
        let lat3_out = Self::lattice_allpass(
            &mut self.lat3_buf, &mut self.lat3_pos, lat2_out, LAT34_COEFF);
        let diffuser_out = Self::lattice_allpass(
            &mut self.lat4_buf, &mut self.lat4_pos, lat3_out, LAT34_COEFF);

        // -- LFO modulation --
        let osc_val = self.osc_phase.sin();
        self.osc_phase += self.osc_step;
        if self.osc_phase >= 2.0 * PI {
            self.osc_phase -= 2.0 * PI;
        }

        let lat5_delay = self.lat5_nom + osc_val * self.mod_depth;
        let lat7_delay = self.lat7_nom - osc_val * self.mod_depth;

        // ═══════ Tank 1 (Left channel) ═══════

        let lat5_in = diffuser_out + self.d4_fb * decay; // feedback from previous D4
        let _lat5_out = Self::decay_diffuser(
            &mut self.tank1_buf, &mut self.tank1_pos, lat5_in, LAT57_COEFF, lat5_delay);

        let d1_tap1 = Self::read_tap(&self.tank1_buf, self.tank1_pos, self.d1_t1);
        let d1_tap2 = Self::read_tap(&self.tank1_buf, self.tank1_pos, self.d1_t2);
        let d1_tap3 = Self::read_tap(&self.tank1_buf, self.tank1_pos, self.d1_t3);
        let d1_tap4 = Self::read_tap(&self.tank1_buf, self.tank1_pos, self.d1_t4);

        // Damping LP
        let damp1_out = d1_tap4 * ONE_MINUS_DAMP + self.damp1_old * DAMPING;
        self.damp1_old = damp1_out;

        let lat6_in = damp1_out * decay;
        let _lat6_out = Self::lattice_allpass_explicit(
            &mut self.tank1b_buf, &mut self.tank1b_pos, lat6_in, lat6_coeff, self.l6_dly);

        let l6_tap1 = Self::read_tap(&self.tank1b_buf, self.tank1b_pos, self.l6_t1);
        let l6_tap2 = Self::read_tap(&self.tank1b_buf, self.tank1b_pos, self.l6_t2);
        let d2_tap1 = Self::read_tap(&self.tank1b_buf, self.tank1b_pos, self.d2_t1);
        let d2_tap2 = Self::read_tap(&self.tank1b_buf, self.tank1b_pos, self.d2_t2);
        let d2_tap3 = Self::read_tap(&self.tank1b_buf, self.tank1b_pos, self.d2_t3);

        // ═══════ Tank 2 (Right channel) ═══════

        let lat7_in = diffuser_out + d2_tap3 * decay; // feedback from current D2
        let _lat7_out = Self::decay_diffuser(
            &mut self.tank2_buf, &mut self.tank2_pos, lat7_in, LAT57_COEFF, lat7_delay);

        let d3_tap1 = Self::read_tap(&self.tank2_buf, self.tank2_pos, self.d3_t1);
        let d3_tap2 = Self::read_tap(&self.tank2_buf, self.tank2_pos, self.d3_t2);
        let d3_tap3 = Self::read_tap(&self.tank2_buf, self.tank2_pos, self.d3_t3);
        let d3_tap4 = Self::read_tap(&self.tank2_buf, self.tank2_pos, self.d3_t4);

        // Damping LP
        let damp2_out = d3_tap4 * ONE_MINUS_DAMP + self.damp2_old * DAMPING;
        self.damp2_old = damp2_out;

        let lat8_in = damp2_out * decay;
        let _lat8_out = Self::lattice_allpass_explicit(
            &mut self.tank2b_buf, &mut self.tank2b_pos, lat8_in, lat6_coeff, self.l8_dly);

        let l8_tap1 = Self::read_tap(&self.tank2b_buf, self.tank2b_pos, self.l8_t1);
        let l8_tap2 = Self::read_tap(&self.tank2b_buf, self.tank2b_pos, self.l8_t2);
        let d4_tap1 = Self::read_tap(&self.tank2b_buf, self.tank2b_pos, self.d4_t1);
        let d4_tap2 = Self::read_tap(&self.tank2b_buf, self.tank2b_pos, self.d4_t2);
        let d4_tap3 = Self::read_tap(&self.tank2b_buf, self.tank2b_pos, self.d4_t3);

        // Store D4 feedback for next iteration (clamped for safety)
        self.d4_fb = d4_tap3.clamp(-4.0, 4.0);

        // ═══════ Output matrix (FxSound mixing) ═══════

        let out1 = -d1_tap2 - l6_tap1 - d2_tap1
            + d3_tap1 + d3_tap3 - l8_tap2 + d4_tap2;
        let out2 = d1_tap1 + d1_tap3 - l6_tap2 + d2_tap2
            - d3_tap2 - l8_tap1 - d4_tap1;

        // Return pure reverb output (scaled, without dry mix)
        let rev_l = (out1 * OUTPUT_SCALE).clamp(-2.0, 2.0);
        let rev_r = (out2 * OUTPUT_SCALE).clamp(-2.0, 2.0);

        (rev_l, rev_r)
    }

    /// Reset all reverb state
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.lat1_buf.fill(0.0);
        self.lat2_buf.fill(0.0);
        self.lat3_buf.fill(0.0);
        self.lat4_buf.fill(0.0);
        self.pre_buf.fill(0.0);
        self.tank1_buf.fill(0.0);
        self.tank1b_buf.fill(0.0);
        self.tank2_buf.fill(0.0);
        self.tank2b_buf.fill(0.0);
        self.lat1_pos = 0;
        self.lat2_pos = 0;
        self.lat3_pos = 0;
        self.lat4_pos = 0;
        self.pre_pos = 0;
        self.tank1_pos = 0;
        self.tank1b_pos = 0;
        self.tank2_pos = 0;
        self.tank2b_pos = 0;
        self.bw_old = 0.0;
        self.damp1_old = 0.0;
        self.damp2_old = 0.0;
        self.osc_phase = 0.0;
        self.d4_fb = 0.0;
    }

    // ── Internal DSP helpers ─────────────────────────────────────────────

    /// Standard lattice allpass (implicit delay from buffer size).
    /// Reads buf[pos], writes buf[pos], advances pos.
    #[inline(always)]
    fn lattice_allpass(buf: &mut [f64], pos: &mut usize, input: f64, coeff: f64) -> f64 {
        let old = buf[*pos];
        let new_input = input - coeff * old;
        buf[*pos] = new_input;
        *pos = (*pos + 1) % buf.len();
        old + coeff * new_input
    }

    /// Standard lattice allpass with EXPLICIT read delay.
    /// Reads from buf[(pos + len - delay) % len], writes buf[pos], advances pos.
    /// For LAT6/LAT8 in shared buffers where delay != buffer size.
    #[inline(always)]
    fn lattice_allpass_explicit(
        buf: &mut [f64], pos: &mut usize, input: f64, coeff: f64, delay: usize,
    ) -> f64 {
        let len = buf.len();
        let old = buf[(*pos + len - delay) % len];
        let new_input = input - coeff * old;
        buf[*pos] = new_input;
        *pos = (*pos + 1) % len;
        old + coeff * new_input
    }

    /// Modulated decay diffuser (with linear interpolation).
    /// Reads from interpolated position, writes buf[pos], advances pos.
    #[inline(always)]
    fn decay_diffuser(
        buf: &mut [f64], pos: &mut usize, input: f64, coeff: f64, delay: f64,
    ) -> f64 {
        let len = buf.len();
        let idly = delay as usize;
        let frac = delay - idly as f64;
        let y1 = buf[(*pos + len - idly) % len];
        let y2 = buf[(*pos + len - idly - 1) % len];
        let dl_out = y1 + (y2 - y1) * frac;
        let dl_in = input + coeff * dl_out;
        buf[*pos] = dl_in;
        *pos = (*pos + 1) % len;
        dl_out - coeff * dl_in
    }

    /// Read a tap at a fixed offset behind the current position.
    #[inline(always)]
    fn read_tap(buf: &[f64], pos: usize, offset: usize) -> f64 {
        buf[(pos + buf.len() - offset) % buf.len()]
    }
}
