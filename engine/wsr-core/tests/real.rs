use std::cmp::Ordering;
use wsr_core::{
    pb::{
        ext80::Ext80,
        numeric,
        real::{Ext, cast},
    },
    state::Globals,
};

#[test]
fn arithmetic_and_persistent_snapshots_retain_the_low_mantissa_bits() {
    let unit = Ext(Ext80 {
        se: 16383 - 63,
        mant: 1 << 63,
    });
    let value = Ext::ONE + unit;
    assert_eq!(value.raw().mant, (1 << 63) + 1);
    assert_eq!(value.to_f64(), 1.0);
    let mut game = Globals::default();
    game.cash[1] = value;
    let json = game.to_json();
    assert!(json.contains("x87:3fff8000000000000001"));
    let mut restored = Globals::default();
    restored.from_json(&json).unwrap();
    assert_eq!(restored.cash[1].raw(), value.raw());
    assert_eq!(restored.cash[1] - Ext::ONE, unit);
    assert!(!game.to_json_portable().contains("x87:"));
}

#[test]
fn comparisons_are_numeric_including_signed_zero_and_unordered_values() {
    assert_eq!(Ext::ZERO, -Ext::ZERO);
    assert_eq!(Ext::ZERO.partial_cmp(&-Ext::ZERO), Some(Ordering::Equal));
    assert_ne!(Ext::NAN, Ext::NAN);
    assert_eq!(Ext::NAN.partial_cmp(&Ext::ONE), None);
    assert!(!(Ext::NAN < Ext::ZERO));
    assert!(Ext::MAX.is_finite());
    assert!(Ext::MAX.to_f64().is_infinite());
    assert!(!Ext::NAN.is_infinite());
    assert!(Ext::INFINITY.is_infinite());
}

#[test]
fn integer_conversions_and_narrow_stores_have_explicit_rounding() {
    let n = (1i64 << 53) + 1;
    assert_eq!(numeric::to_quad(Ext::from_i64(n)), n);
    assert_eq!(numeric::to_quad(Ext::from_f64(2.5)), 2);
    assert_eq!(numeric::to_quad(Ext::from_f64(3.5)), 4);
    assert_eq!(cast::<i64, _>(Ext::from_f64(-3.9)), -3);
    assert_eq!(cast::<i64, _>(Ext::NAN), 0);
    assert_eq!(cast::<i64, _>(Ext::INFINITY), i64::MAX);
    assert_eq!(numeric::to_quad(Ext::NAN), i64::MIN);
    let midpoint = Ext(Ext80 {
        se: 16383,
        mant: (1 << 63) + (1 << 39),
    });
    assert_eq!(cast::<f32, _>(midpoint).to_bits(), 1.0f32.to_bits());
    assert_eq!(
        cast::<f32, _>(
            midpoint
                + Ext(Ext80 {
                    se: 16383 - 63,
                    mant: 1 << 63
                })
        )
        .to_bits(),
        1.0f32.to_bits() + 1
    );
}

#[test]
fn roots_and_integral_rounding_keep_extended_range_and_signed_zero() {
    assert_eq!(Ext::from_i64(4).sqrt(), Ext::from_i64(2));
    let root = Ext::from_i64(2).sqrt();
    assert_eq!(
        root.raw(),
        Ext80 {
            se: 0x3fff,
            mant: 0xb504f333f9de6484
        }
    );
    assert_eq!((-Ext::ZERO).sqrt().raw(), (-Ext::ZERO).raw());
    assert!(Ext::from_i64(-1).sqrt().is_nan());
    assert_eq!(Ext::from_f64(-0.25).floor(), Ext::from_i64(-1));
    assert_eq!(Ext::from_f64(-0.25).ceil(), Ext::ZERO);
    assert!(Ext::MAX.sqrt().is_finite());
    let large_odd_integer = Ext::from_u64((1u64 << 63) + 1);
    assert_eq!(large_odd_integer.round(), large_odd_integer);
    assert_eq!((-large_odd_integer).round(), -large_odd_integer);
    assert_eq!(Ext::from_f64(-0.5).round(), -Ext::ONE);
    assert_eq!((-Ext::ZERO).round().raw(), (-Ext::ZERO).raw());
}

#[test]
fn roots_match_original_x87_instruction_vectors() {
    let parse = |text: &str| Ext80 {
        se: u16::from_str_radix(&text[..4], 16).unwrap(),
        mant: u64::from_str_radix(&text[4..], 16).unwrap(),
    };
    let mut checked = 0;
    for line in include_str!("fixtures/real.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let a = Ext(parse(fields[1]));
        assert_eq!(a.sqrt().raw(), parse(fields[2]), "{line}");
        checked += 1;
    }
    assert_eq!(checked, 2500);
}

#[test]
fn remainders_do_not_round_a_large_quotient_through_double() {
    let huge = Ext(Ext80::ONE.scalbn(10000));
    assert_eq!(huge % Ext::from_i64(3), Ext::ONE);
    assert_eq!(-huge % Ext::from_i64(3), -Ext::ONE);
    assert_eq!(Ext::from_f64(7.5) % Ext::from_i64(2), Ext::from_f64(1.5));
    assert_eq!((-huge % Ext::ONE).raw(), (-Ext::ZERO).raw());
    assert!((huge % Ext::ZERO).is_nan());
}

#[test]
fn powers_preserve_small_increments_and_extended_exponents() {
    let increment = Ext(Ext80::ONE.scalbn(-63));
    assert!((Ext::ONE + increment).powf(Ext::from_i64(3)) > Ext::ONE);
    assert_eq!(
        Ext::from_i64(2).powf(Ext::from_i64(16000)),
        Ext(Ext80::ONE.scalbn(16000))
    );
    assert_eq!(Ext::from_i64(-2).powf(Ext::from_i64(3)), Ext::from_i64(-8));
    assert_eq!(Ext::from_i64(-2).powf(Ext::from_f64(2.5)), Ext::from_i64(4));
    assert_eq!(Ext::from_i64(4).powf(Ext::from_f64(0.5)), Ext::from_i64(2));
    assert_eq!(
        Ext::from_i64(2).powf(Ext::from_i64(-4)),
        Ext::from_f64(0.0625)
    );
}

#[test]
fn fractional_powers_agree_with_an_independent_high_precision_reference() {
    let parse = |s: &str| {
        Ext(Ext80 {
            se: u16::from_str_radix(&s[..4], 16).unwrap(),
            mant: u64::from_str_radix(&s[4..], 16).unwrap(),
        })
    };
    let mut worst = Ext::ZERO;
    for line in include_str!("fixtures/power.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split('\t').collect();
        let expected = parse(p[2]);
        let result = parse(p[0]).powf(parse(p[1]));
        let error = (result - expected).abs() / expected;
        worst = worst.max(error);
        assert!(
            error <= Ext::EPSILON * Ext::from_i64(128),
            "{line}: {error}"
        );
    }
    eprintln!("worst relative power error: {worst:e}");
}
