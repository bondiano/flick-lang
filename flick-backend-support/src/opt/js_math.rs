//! Math functions with the bits JS gives, for the backends that evaluate
//! them without a JS engine: the literal evaluator and the memory machine.

/// The natural logarithm as fdlibm's `__ieee754_log` (`e_log.c`) computes it,
/// the bits JS's `Math.log` gives: a `value` of `2^exponent * (1 + frac)`, with `1 + frac`
/// within `[sqrt(2)/2, sqrt(2))`, is `exponent * ln2 + log(1 + frac)`, and `log(1 + frac)`
/// a polynomial in `ratio = frac / (2 + frac)`.
pub fn js_log(value: f64) -> f64 {
    // fdlibm's constants, by their bits
    const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
    const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
    const TWO54: f64 = f64::from_bits(0x4350_0000_0000_0000);
    const THIRD: f64 = f64::from_bits(0x3fd5_5555_5555_5555);
    const LG: [f64; 7] = [
        f64::from_bits(0x3fe5_5555_5555_5593),
        f64::from_bits(0x3fd9_9999_9997_fa04),
        f64::from_bits(0x3fd2_4924_9422_9359),
        f64::from_bits(0x3fcc_71c5_1d8e_78af),
        f64::from_bits(0x3fc7_4664_96cb_03de),
        f64::from_bits(0x3fc3_9a09_d078_c69f),
        f64::from_bits(0x3fc2_f112_df3e_5244),
    ];
    // the high and low words of fdlibm's `__HI` and `__LO`
    let high = |value: f64| u32::try_from(value.to_bits() >> 32).map_or(0, u32::cast_signed);
    let mut value = value;
    let mut hx = high(value);
    let lx = u32::try_from(value.to_bits() & 0xffff_ffff).unwrap_or(0);
    let mut exponent: i32 = 0;
    if hx < 0x0010_0000 {
        if ((hx & 0x7fff_ffff).cast_unsigned() | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        // a subnormal, scaled to a normal number
        exponent -= 54;
        value *= TWO54;
        hx = high(value);
    }
    if hx >= 0x7ff0_0000 {
        return value + value;
    }
    exponent += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let half = (hx + 0x95f64) & 0x0010_0000;
    // value, or value / 2, into [sqrt(2)/2, sqrt(2))
    let high_word = (hx | (half ^ 0x3ff0_0000)).cast_unsigned();
    value = f64::from_bits((u64::from(high_word) << 32) | (value.to_bits() & 0xffff_ffff));
    exponent += half >> 20;
    let frac = value - 1.0;
    let scaled = f64::from(exponent);
    if (0x000f_ffff & (2 + hx)) < 3 {
        // |frac| < 2^-20
        if frac == 0.0 {
            return if exponent == 0 {
                0.0
            } else {
                scaled * LN2_HI + scaled * LN2_LO
            };
        }
        let poly = frac * frac * (0.5 - THIRD * frac);
        return if exponent == 0 {
            frac - poly
        } else {
            scaled * LN2_HI - ((poly - scaled * LN2_LO) - frac)
        };
    }
    let ratio = frac / (2.0 + frac);
    let ratio2 = ratio * ratio;
    let ratio4 = ratio2 * ratio2;
    let t1 = ratio4 * (LG[1] + ratio4 * (LG[3] + ratio4 * LG[5]));
    let t2 = ratio2 * (LG[0] + ratio4 * (LG[2] + ratio4 * (LG[4] + ratio4 * LG[6])));
    let poly = t2 + t1;
    if ((hx - 0x6147a) | (0x6b851 - hx)) > 0 {
        let hfsq = 0.5 * frac * frac;
        if exponent == 0 {
            frac - (hfsq - ratio * (hfsq + poly))
        } else {
            scaled * LN2_HI - ((hfsq - (ratio * (hfsq + poly) + scaled * LN2_LO)) - frac)
        }
    } else if exponent == 0 {
        frac - ratio * (frac - poly)
    } else {
        scaled * LN2_HI - ((ratio * (frac - poly) - scaled * LN2_LO) - frac)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::E;

    use super::js_log;

    /// What V8's `Math.log` gives, as bits.
    #[test]
    fn js_log_has_the_bits_of_js() {
        let cases: [(f64, u64); 8] = [
            (2.0, 0x3fe6_2e42_fefa_39ef),
            (10.0, 0x4002_6bb1_bbb5_5516),
            (0.5, 0xbfe6_2e42_fefa_39ef),
            (1e300, 0x4085_9634_47f8_7fb5),
            (5e-324, 0xc087_4385_446d_71c3),
            (1.0, 0),
            (E, 0x3ff0_0000_0000_0000),
            (0.999_999_999_999, 0xbd71_9780_0000_09ac),
        ];
        for (x, bits) in cases {
            assert_eq!(js_log(x).to_bits(), bits, "log {x}");
        }
        assert!(js_log(-1.0).is_nan() && js_log(f64::NAN).is_nan());
        let bits = [0.0, -0.0, f64::INFINITY].map(|x| js_log(x).to_bits());
        assert_eq!(
            bits,
            [f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY].map(f64::to_bits)
        );
    }
}
