# Simulation numbers

`pb::real::Ext` wraps the raw x87 format in `pb::ext80::Ext80`: a sign/exponent word and a
64-bit significand. `pb::types::Ext` reexports it. It implements numeric comparisons: positive
and negative zero compare equal, NaNs are unordered. `Ext80` itself retains raw-bit equality for
fixtures, exact checkpoints and diagnostics. The runtime adds no external dependencies.

Addition, subtraction, multiplication, division, square roots, remainders, integral rounding and
integer conversion run in software extended precision. Primitive `as` conversions are replaced
by explicit `pb::real::cast` calls where the source may be EXT. These casts truncate and saturate
like Rust; PowerBASIC integer stores instead use `pb::numeric::to_integer`, `to_long` and
`to_quad`, which implement nearest-even FISTP and its integer-indefinite overflow result.

SINGLE and DOUBLE remain native `f32` and `f64` in generated state. Original local DOUBLE spills
use `pb::numeric::double`: round to DOUBLE, then reload into EXT. SINGLE constants retain their
32-bit bits before widening. Moving or removing these boundaries changes behavior, including
strict threshold comparisons and the last digits of record strings.

`pb::numeric` supplies simulation-facing parsing and formatting. The original `pb::numfmt`
interfaces remain available to raw runtime fixtures. CURRENCY and CURRENCYX retain exact scaled
integer storage and pass through extended intermediates on load/store. Browser summary numbers
and chart JSON convert at the presentation boundary; they are not used to restore simulation state.

## Powers and reference limits

Small integer powers follow PowerBASIC's left-to-right squaring sequence. Fractional powers use
range-reduced logarithm and exponential series in EXT, without a host-Double fallback. Negative
bases round the exponent to an integer, as PB_Pow does. This is not a claim of bit equality with
every x87 processor's transcendental instructions. The independent 100-digit Decimal fixture
checks 512 fractional powers; observed relative error is below 2e-18 for those inputs.

[Unicorn's FYL2X, F2XM1 and FPREM helpers](https://github.com/unicorn-engine/unicorn/blob/master/qemu/target/i386/fpu_helper.c)
narrow through host Double. They cannot establish full extended accuracy. Square-root fixtures use the emulated FSQRT instruction; remainder tests use
exact integer identities, including exponents beyond Double's range. Fixture generators are in
`tools/emu/ext80.py` (`fixture`, `real-fixture`, `power-fixture`).

## Snapshots and saves

`Globals::to_json()` preserves every EXT bit. Values exactly representable in DOUBLE keep the
legacy JSON number representation. Other values use `"x87:SSSSMMMMMMMMMMMMMMMM"`, with four
hexadecimal sign/exponent digits followed by sixteen significand digits. The reader accepts both
representations, including legacy snapshots. `to_json_portable()` is explicitly approximate and
intended for presentation or tools that require ordinary JSON numbers.

The Python emulator observer uses the same representation. Both campaign and per-routine
comparators understand it without first converting to DOUBLE. `campaign.py --exact-ext` also
checks signed zero and every low mantissa bit; default diagnostic tolerances remain unchanged.
Integer, CURRENCY, string and durable file checks remain exact.

The original saved-game format is unchanged. `WRITE #` still uses PB's 15 significant decimal
digits, so saving/reloading is not a lossless checkpoint of every low bit. The retained first-save
case now matches all original bytes in a 101-command replay covering reload, the player handoff and
subsequent ticks. Exact multi-year campaign parity remains unproven; raw comparison still exposes
low-bit differences.

A decimal such as `1e999` is finite in EXT. It overflows a DOUBLE store, but must not be rejected
merely because a diagnostic display cannot represent it. PB's decimal parser clamps exponents
beyond its accepted EXT range to 2^127; corrupt-save fixtures account for that behavior.

## Browser cost

`node web/engine-benchmark.mjs web/wsr.wasm /path/to/GAME01.DAT extracted/app 1000` measures
real WASM callbacks with deterministic time and host replies. It asserts that the simulated date
advances. On the local January-to-March fixture, the pre-migration build took about 1.1 ms median /
2.8 ms p95 per tick; EXT took about 1.7 ms median / 7.9 ms p95. These are local measurements,
not cross-browser performance guarantees. Uncompressed WASM grew from approximately 7.8 MiB
to 10 MiB. Both run in the worker, leaving the input controls on
the page responsive. Occasional heavy callbacks can take substantially longer than the p95.
