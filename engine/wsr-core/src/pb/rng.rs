//! PowerBASIC 8 random number generator (`RND`, `RND(a, b)`, `RANDOMIZE`), bit-exact with WSR.EXE.
//!
//! Original routines (image base 0x400000):
//!
//! | address    | name            | what it does                                                        |
//! |------------|-----------------|---------------------------------------------------------------------|
//! | `0x6b735d` | runtime init    | zero-fills `[0x6c3010,0x6c327c)`, fills `[0x6c327c,0x6c328c)` with `0xff` |
//! | `0x6bc3d0` | `PB_Randomize`  | `fstp dword [0x6c3280]` - the seed (ST0) rounded to an IEEE single; its *bit pattern* becomes the state |
//! | `0x6bc3d7` | `PB_Rnd`        | `RndNext`, then `RndVal`                                              |
//! | `0x6bc3df` | `RND(n)`        | n>0: like RND; n<0: state = f32 bits of n, then RndNext; n=0: repeat last value (never called by the game) |
//! | `0x6bc3fe` | `PB_RndInt`     | `RND(a, b)`, EAX=a, ECX=b, result in EAX                              |
//! | `0x6bc473` | `pbrt_RndNext`  | `state = state * 0x08088405 + 1` (mod 2^32, done with 16-bit ops in `0x6bc439`); `last = state >> 1` |
//! | `0x6bc4a6` | `RndVal`        | `fild dword [last]` ; `fscale` by `fild word [0x6b712a]` (= -31) -> `last * 2^-31` |
//! | `0x6bd659` | `PB_Timer`      | `fild (h*3600000+m*60000+s*1000+ms)` ; `fidiv 1000` (GetLocalTime)    |
//!
//! The LCG is Borland's (`RandSeed * 134775813 + 1`). State dword `0x6c3280`, last-output dword `0x6c327c`.
//!
//! Startup state: the init routine leaves both dwords at `0xffffffff`; nothing else touches them before user
//! code runs, so [`Rng::default`] has `state = 0xffffffff`. (`last = 0xffffffff` would only be visible through
//! `RND(0)`, which the game never uses; it would return `-2^-31`.)
//!
//! Precision: `RND` produces `last * 2^-31` with `last < 2^31`, a 31-bit integer scaled by a power of two,
//! which is exactly representable in f64, so the f64 result equals the original 80-bit ST0 exactly.
//! `RND(a, b)` is computed on the x87 (`fild qword (range+1)`, `fmulp`, `fiadd min`, `frndint` in round-down
//! mode with CW `0x173f`, `fistp`). All intermediate values fit in the 64-bit x87 mantissa (n*(range+1) < 2^63,
//! plus `min * 2^31` stays < 2^64 in magnitude), so the original is exact and equals the integer formula
//! `min + ((n * (range+1)) >> 31)` used here. Doing it in f64 would NOT be exact for large ranges.
//!
//! The game's call sites: `RND` (0x6bc3d7) 1900 times and `RND(a, b)` 317 times, both without extra register
//! arguments; `RND(n)` (0x6bc3df) is never called. `RANDOMIZE` is called 4 times: three times as
//! `RANDOMIZE TIMER` (0x4014d6 in WinMain, 0x472ac4, 0x47445e: `call PB_Timer; call PB_Randomize`) and once as
//! `RANDOMIZE seed&` with an integer variable (0x47bcff: `fild dword [0x6c3c28]`; the variable is clamped to
//! 10 if > 32500 just before). For TIMER use [`Rng::randomize_timer`], which reproduces the 80-bit quotient.

/// LCG multiplier (Borland `Random`), as assembled from the shift/add sequence at 0x6bc439.
pub const MULTIPLIER: u32 = 0x0808_8405;

/// PowerBASIC RNG state (dword at 0x6c3280).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rng {
    state: u32,
}

impl Default for Rng {
    /// Runtime startup state (0x6b735d fills the state dword with `0xff` bytes).
    fn default() -> Self {
        Rng { state: 0xffff_ffff }
    }
}

impl Rng {
    /// Creates an RNG with the given raw state dword.
    pub fn from_state(state: u32) -> Self {
        Rng { state }
    }

    /// Raw state dword (0x6c3280).
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Sets the raw state dword (0x6c3280).
    pub fn set_state(&mut self, state: u32) {
        self.state = state;
    }

    /// `RANDOMIZE seed` (0x6bc3d0): the state becomes the IEEE single bit pattern of `seed`, rounded
    /// to nearest-even (CW 0x133f), overflowing to +-inf and producing subnormals like the x87 with masked
    /// exceptions. For an 80-bit seed that is not an exact f64 the double rounding (80->64->32) could differ
    /// from the original's direct 80->32 rounding; integer seeds and f64 seeds are exact. Use
    /// [`Rng::randomize_timer`] for `RANDOMIZE TIMER`.
    pub fn randomize(&mut self, seed: f64) {
        self.state = (seed as f32).to_bits();
    }

    /// `RANDOMIZE TIMER` where TIMER was read at `ms_since_midnight` (h*3600000 + m*60000 + s*1000 + ms).
    pub fn randomize_timer(&mut self, ms_since_midnight: u32) {
        self.state = timer_seed_bits(ms_since_midnight);
    }

    /// LCG step (0x6bc473); returns the new 31-bit output (`state >> 1`, stored at 0x6c327c).
    pub fn next_raw(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(1);
        self.state >> 1
    }

    /// `RND` (0x6bc3d7): value in `[0, 1)` = `(state >> 1) * 2^-31`. Exact (the original's 80-bit ST0
    /// holds the same value).
    pub fn rnd(&mut self) -> f64 {
        self.next_raw() as f64 * (1.0 / 2147483648.0)
    }

    /// `RND(a, b)` (0x6bc3fe): uniformly distributed integer between `a` and `b` inclusive (either order).
    pub fn rnd_range(&mut self, a: i32, b: i32) -> i32 {
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        let span = (hi.wrapping_sub(lo) as u32 as u64) + 1; // 1 ..= 2^32 (the 'inc ebx / adc' qword)
        let n = self.next_raw() as u64;
        // exact x87 result: floor(n * 2^-31 * span + lo) = lo + floor(n * span / 2^31)
        (lo as i64 + ((n * span) >> 31) as i64) as i32
    }
}

/// Bit pattern of `TIMER` (0x6bd659) after `fstp dword`, i.e. what `RANDOMIZE TIMER` stores.
///
/// TIMER is `ms / 1000` computed by `fidiv` at 64-bit precision (round to nearest-even), then rounded again
/// to 24 bits by `fstp dword`. Both roundings are reproduced exactly with integer arithmetic.
pub fn timer_seed_bits(ms_since_midnight: u32) -> u32 {
    let (q, k) = match timer_ext(ms_since_midnight) {
        None => return 0,
        Some(v) => v,
    };
    // round 64-bit significand to 24 bits, nearest-even
    let mut q24 = q >> 40;
    let r = q & ((1u64 << 40) - 1);
    let half = 1u64 << 39;
    if r > half || (r == half && q24 & 1 == 1) {
        q24 += 1;
    }
    let v = q24 as f64 * 2f64.powi(40 - k);
    (v as f32).to_bits()
}

/// TIMER (0x6bd659) as the x87 computes it: `ms / 1000` rounded to a 64-bit significand.
/// Returns `(significand, k)` with value `significand * 2^-k` and `2^63 <= significand < 2^64`, or `None`
/// for 0.
pub fn timer_ext(ms_since_midnight: u32) -> Option<(u64, i32)> {
    let ms = ms_since_midnight as u128;
    if ms == 0 {
        return None;
    }
    // choose k so that 2^63 <= ms * 2^k / 1000 < 2^64
    let mut k: i32 = 0;
    while (ms << k) / 1000 < (1u128 << 63) {
        k += 1;
    }
    let num = ms << k;
    let mut q = num / 1000;
    let rem = num % 1000;
    if rem * 2 > 1000 || (rem * 2 == 1000 && q & 1 == 1) {
        q += 1;
    }
    if q == 1u128 << 64 {
        q >>= 1;
        k -= 1;
    }
    Some((q as u64, k))
}

/// TIMER as an f64 (`ms / 1000` rounded once to 53 bits). Not bit-identical to the 80-bit TIMER, but checked
/// exhaustively for every millisecond of the day: `randomize(timer_f64(ms))` gives the same state as
/// [`Rng::randomize_timer`]`(ms)`. Other uses of TIMER (e.g. elapsed-time arithmetic) may differ in the last bits.
pub fn timer_f64(ms_since_midnight: u32) -> f64 {
    ms_since_midnight as f64 / 1000.0
}
