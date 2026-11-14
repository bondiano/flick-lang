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
    let text = d.is_human_readable();
    // CBOR permits any IEEE width for a Float, including the f16/f32 forms
    // our writer chooses when they represent the value exactly.
    d.deserialize_any(Float { text })
}

struct Float {
    text: bool,
}

impl<'de> Visitor<'de> for Float {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number, \"NaN\", \"inf\" or \"-inf\"")
    }

    fn visit_f64<E: de::Error>(self, x: f64) -> Result<f64, E> {
        Ok(x)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_i64<E: de::Error>(self, x: i64) -> Result<f64, E> {
        if !self.text {
            return Err(E::invalid_type(de::Unexpected::Signed(x), &self));
        }
        Ok(x as f64)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_u64<E: de::Error>(self, x: u64) -> Result<f64, E> {
        if !self.text {
            return Err(E::invalid_type(de::Unexpected::Unsigned(x), &self));
        }
        Ok(x as f64)
    }

    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<f64, A::Error> {
        #[cfg(feature = "cbor")]
        {
            use ciborium::tag::Captured;
            use serde::Deserialize;
            use serde::de::value::EnumAccessDeserializer;
            #[derive(Deserialize)]
            struct Number(#[serde(with = "crate::float")] f64);
            let Captured(_, Number(value)) =
                Captured::deserialize(EnumAccessDeserializer::new(data))?;
            Ok(value)
        }
        #[cfg(not(feature = "cbor"))]
        {
            let _ = data;
            Err(de::Error::invalid_type(de::Unexpected::Enum, &self))
        }
    }

    fn visit_str<E: de::Error>(self, x: &str) -> Result<f64, E> {
        if !self.text {
            return Err(E::invalid_type(de::Unexpected::Str(x), &self));
        }
        match x {
            "NaN" => Ok(f64::NAN),
            "inf" => Ok(f64::INFINITY),
            "-inf" => Ok(f64::NEG_INFINITY),
            _ => Err(E::invalid_value(de::Unexpected::Str(x), &self)),
        }
    }
}

#[cfg(all(test, feature = "cbor"))]
mod tests {
    use cbor4ii::serde::from_slice;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Number(#[serde(with = "crate::float")] f64);

    #[test]
    fn binary_float_widths_and_tags_keep_their_existing_meaning() {
        for bytes in [
            vec![0xf9, 0x3e, 0x00],
            vec![0xfa, 0x3f, 0xc0, 0x00, 0x00],
            vec![0xfb, 0x3f, 0xf8, 0, 0, 0, 0, 0, 0],
            vec![0xd8, 42, 0xf9, 0x3e, 0x00],
        ] {
            let Number(value) = ciborium::from_reader(&bytes[..]).expect("CBOR Float");
            assert_eq!(value.to_bits(), 1.5f64.to_bits());
        }
        for bytes in [vec![1], vec![0x63, b'i', b'n', b'f']] {
            assert!(ciborium::from_reader::<Number, _>(&bytes[..]).is_err());
            assert!(from_slice::<Number>(&bytes).is_err());
        }
    }
}
