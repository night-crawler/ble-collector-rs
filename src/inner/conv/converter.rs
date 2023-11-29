use std::fmt::{Display, Formatter};

use bounded_integer::{BoundedI8, BoundedU8};
use num_bigint::{BigInt, BigUint};
use num_traits::{FromBytes, ToPrimitive};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConversionError {
    #[error("Unexpected value length: expected {expected}, actual {actual}")]
    LenMismatch { expected: usize, actual: usize },

    #[error("Big integer conversion error: {0}")]
    BigIntCastError(BigInt),

    #[error("Big unsigned integer conversion error: {0}")]
    BigUintCastError(BigUint),

    #[error("Utf8 conversion error: {0:?}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub(crate) enum Converter {
    #[default]
    Raw,
    Utf8,
    F32,
    Signed {
        l: BoundedU8<0, 8>,
        m: BoundedI8<-10, 10>,
        d: i32,
        b: i32,
    },
    Unsigned {
        l: BoundedU8<0, 8>,
        m: BoundedI8<-10, 10>,
        d: i32,
        b: i32,
    },
}

impl Display for Converter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Converter::Raw => write!(f, "Raw"),
            Converter::Utf8 => write!(f, "Utf8"),
            Converter::Signed { l, m, d, b } => write!(f, "Signed[{l}]({m} {d} {b})",),
            Converter::Unsigned { l, m, d, b } => write!(f, "Unsigned[{l}]({m} {d} {b})",),
            Converter::F32 => write!(f, "F32")
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CharacteristicValue {
    Raw(Vec<u8>),
    Utf8(String),
    I64(i64),
    F64(f64),
}

impl Serialize for CharacteristicValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            CharacteristicValue::Raw(value) => serializer.serialize_bytes(value),
            CharacteristicValue::Utf8(value) => serializer.serialize_str(value),
            CharacteristicValue::I64(value) => serializer.serialize_i64(*value),
            CharacteristicValue::F64(value) => serializer.serialize_f64(*value),
        }
    }
}

impl Display for CharacteristicValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacteristicValue::Raw(value) => write!(f, "{:?}", value),
            CharacteristicValue::Utf8(value) => write!(f, "{}", value),
            CharacteristicValue::I64(value) => write!(f, "{}", value),
            CharacteristicValue::F64(value) => write!(f, "{}", value),
        }
    }
}

fn compute_r(
    value: i64,
    multiplier: i8,
    decimal_exponent: i32,
    binary_exponent: i32,
) -> CharacteristicValue {
    if decimal_exponent >= 0 && binary_exponent >= 0 {
        let result = value
            * (multiplier as i64)
            * 10i64.pow(decimal_exponent as u32)
            * 2i64.pow(binary_exponent as u32);
        return CharacteristicValue::I64(result);
    }

    let result = value as f64
        * (multiplier as f64)
        * 10f64.powi(decimal_exponent)
        * 2f64.powi(binary_exponent);
    CharacteristicValue::F64(result)
}

impl Converter {
    fn check_length(&self, value: &[u8]) -> Result<(), ConversionError> {
        match self {
            Converter::Signed { l, .. } | Converter::Unsigned { l, .. } => {
                if value.len() != usize::from(*l) {
                    return Err(ConversionError::LenMismatch {
                        expected: usize::from(*l),
                        actual: value.len(),
                    });
                }
                Ok(())
            },
            Converter::F32 => {
                if value.len() != 4 {
                    return Err(ConversionError::LenMismatch {
                        expected: 4,
                        actual: value.len(),
                    });
                }
                Ok(())
            },
            _ => Ok(()),
        }
    }
    pub(crate) fn convert(&self, value: Vec<u8>) -> Result<CharacteristicValue, ConversionError> {
        // assume i64 will suffice for all conversions for now
        match self {
            Converter::F32 => {
                self.check_length(&value)?;
                let value = f32::from_le_bytes(<[u8; 4]>::try_from(value).unwrap());
                Ok(CharacteristicValue::F64(value as f64))
            }
            Converter::Raw => Ok(CharacteristicValue::Raw(value)),
            Converter::Utf8 => {
                let result = String::from_utf8(value)?;
                Ok(CharacteristicValue::Utf8(result))
            }
            &Converter::Signed { m, d, b, .. } => {
                self.check_length(&value)?;
                let value = BigInt::from_le_bytes(&value);
                let value = if let Some(value) = value.to_i64() {
                    value
                } else {
                    return Err(ConversionError::BigIntCastError(value));
                };

                Ok(compute_r(value, i8::from(m), d, b))
            }
            &Converter::Unsigned { m, d, b, .. } => {
                self.check_length(&value)?;
                let value = BigUint::from_le_bytes(&value);
                let value = if let Some(value) = value.to_i64() {
                    value
                } else {
                    return Err(ConversionError::BigUintCastError(value));
                };

                Ok(compute_r(value, i8::from(m), d, b))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    fn ble_serialize(
        num: f32,
        multiplier: i32,
        decimal_exponent: i32,
        binary_exponent: i32,
    ) -> f32 {
        let mut result = num / (multiplier as f32);
        result /= 10f32.powi(decimal_exponent);
        result /= 2f32.powi(binary_exponent);

        result
    }

    #[test]
    fn test() {
        let converter = super::Converter::Signed {
            l: BoundedU8::new(2).unwrap(),
            m: BoundedI8::new(1).unwrap(),
            d: 0,
            b: -6,
        };

        let encoded = ble_serialize(-12.4f32, 1, 0, -6) as i16;
        let encoded_bytes = encoded.to_le_bytes().to_vec();
        let CharacteristicValue::F64(result) = converter.convert(encoded_bytes).unwrap() else {
            panic!("Unexpected result");
        };

        approx_eq!(f64, result, -12.4f64, ulps = 2);
    }
}
