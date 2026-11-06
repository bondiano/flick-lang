//! A float in a text format: a number when it is finite, else `"NaN"`,
//! `"inf"` or `"-inf"`, which JSON has no number for. A binary format
//! keeps the float as it is.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};

#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn serialize<S: Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    match *x {
        x if x.is_finite() || !s.is_human_readable() => s.serialize_f64(x),
        x if x.is_nan() => s.serialize_str("NaN"),
        x if x > 0.0 => s.serialize_str("inf"),
        _ => s.serialize_str("-inf"),
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    // as `serialize` wrote it: a string only in a text format
    if d.is_human_readable() {
        d.deserialize_any(Float)
    } else {
        d.deserialize_f64(Float)
    }
}

struct Float;

impl Visitor<'_> for Float {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number, \"NaN\", \"inf\" or \"-inf\"")
    }

    fn visit_f64<E: de::Error>(self, x: f64) -> Result<f64, E> {
        Ok(x)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_i64<E: de::Error>(self, x: i64) -> Result<f64, E> {
        Ok(x as f64)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_u64<E: de::Error>(self, x: u64) -> Result<f64, E> {
        Ok(x as f64)
    }

    fn visit_str<E: de::Error>(self, x: &str) -> Result<f64, E> {
        match x {
            "NaN" => Ok(f64::NAN),
            "inf" => Ok(f64::INFINITY),
            "-inf" => Ok(f64::NEG_INFINITY),
            _ => Err(E::invalid_value(de::Unexpected::Str(x), &self)),
        }
    }
}
