#[cfg(feature = "bytemuck")]
use bytemuck::{Pod, Zeroable};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    ops::Div,
};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "bytemuck", derive(Pod, Zeroable))]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct Fixed<T, const EXP: u32>(T);

pub type Fixed6D32 = Fixed<i32, 6>;

pub trait FixedDecimalBaseNum:
    Sized + Copy + Clone + Eq + PartialEq + Ord + PartialOrd + Div + fmt::Display
{
    const ZERO: Self;
    const TEN: Self;

    fn split(self, exp: u32) -> (Self, Self, bool);
    fn join(self, frac: Self, exp: u32, neg: bool) -> Self;
    fn from_f32(value: f32) -> Self;
    fn from_f64(value: f64) -> Self;
    fn as_f32(self) -> f32;
    fn as_f64(self) -> f64;
    fn as_u8(self) -> u8;
}

impl<T: FixedDecimalBaseNum, const EXP: u32> Fixed<T, EXP> {
    pub const ZERO: Self = Self(T::ZERO);

    #[inline]
    pub fn from_f32(value: f32) -> Self {
        let value = value * (10u32.pow(EXP) as f32);
        Self(T::from_f32(value))
    }

    #[inline]
    pub fn from_f64(value: f64) -> Self {
        let value = value * (10u64.pow(EXP) as f64);
        Self(T::from_f64(value))
    }

    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0.as_f32() / (10u32.pow(EXP) as f32)
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0.as_f64() / (10u64.pow(EXP) as f64)
    }

    #[inline]
    pub fn convert<const NEW_EXP: u32>(self) -> Fixed<T, NEW_EXP> {
        let (int, frac, neg) = self.0.split(EXP);
        Fixed(int.join(frac, NEW_EXP, neg))
    }
}

impl<T: FixedDecimalBaseNum, const EXP: u32> fmt::Display for Fixed<T, EXP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = format!("{}", self.0);
        let negative = s.starts_with('-');
        let minus_ofs = if negative { 1 } else { 0 };

        while s.len() <= EXP as usize + minus_ofs {
            s.insert(minus_ofs, '0');
        }
        s.insert(s.len() - EXP as usize, '.');
        f.write_str(&s)
    }
}

impl<T: FixedDecimalBaseNum, const EXP: u32> fmt::Debug for Fixed<T, EXP> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<T: FixedDecimalBaseNum, const EXP: u32> From<f32> for Fixed<T, EXP> {
    #[inline(always)]
    fn from(value: f32) -> Self {
        Self::from_f32(value)
    }
}

impl<T: FixedDecimalBaseNum, const EXP: u32> From<f64> for Fixed<T, EXP> {
    #[inline(always)]
    fn from(value: f64) -> Self {
        Self::from_f64(value)
    }
}

macro_rules! impl_base_num {
  ($t:ty $([$abs:ident])?) => {

    impl FixedDecimalBaseNum for $t {
      const ZERO: Self = 0;
      const TEN: Self = 10;

      #[inline(always)]
      fn split(self, exp: u32) -> (Self, Self, bool) {
        let value = self;
        let neg = false $(
            || value < 0;
          let value = value.$abs()
        )?;
        let factor = Self::TEN.pow(exp);
        (self / factor, value % factor, neg)
      }

      #[inline(always)]
      fn join(self, frac: Self, exp: u32, _neg: bool) -> Self {
        let factor = Self::TEN.pow(exp);
        let value = self * factor + frac;
        $(
          if _neg {
            return -value.$abs();
          }
        )?
        value
      }

      #[inline(always)]
      fn from_f32(value: f32) -> Self {
        value as Self
      }

      #[inline(always)]
      fn from_f64(value: f64) -> Self {
        value as Self
      }


      #[inline(always)]
      fn as_f32(self) -> f32 {
        self as f32
      }

      #[inline(always)]
      fn as_f64(self) -> f64 {
        self as f64
      }

      #[inline(always)]
      fn as_u8(self) -> u8 {
        self as u8
      }
    }

  };
}

impl_base_num!(i16[abs]);
impl_base_num!(i32[abs]);
impl_base_num!(i64[abs]);
impl_base_num!(u16);
impl_base_num!(u32);
impl_base_num!(u64);

#[cfg(test)]
mod tests {
    use crate::fixed::Fixed6D32;

    #[test]
    fn test_display() {
        assert_eq!("-1230.000000", format!("{}", Fixed6D32::from_f32(-1230.)));
        assert_eq!("-123.000000", format!("{}", Fixed6D32::from_f32(-123.)));
        assert_eq!("-12.300000", format!("{}", Fixed6D32::from_f32(-12.3)));
        assert_eq!("-1.230000", format!("{}", Fixed6D32::from_f32(-1.23)));
        assert_eq!("-0.123000", format!("{}", Fixed6D32::from_f32(-0.123)));
        assert_eq!("-0.012300", format!("{}", Fixed6D32::from_f32(-0.0123)));
        assert_eq!("-0.001230", format!("{}", Fixed6D32::from_f32(-0.00123)));
        assert_eq!("-0.000123", format!("{}", Fixed6D32::from_f32(-0.000123)));
        assert_eq!("-0.000012", format!("{}", Fixed6D32::from_f32(-0.0000123)));
        assert_eq!("-0.000001", format!("{}", Fixed6D32::from_f32(-0.00000123)));
        assert_eq!("0.000000", format!("{}", Fixed6D32::from_f32(-0.000000123)));
        assert_eq!("1230.000000", format!("{}", Fixed6D32::from_f32(1230.)));
        assert_eq!("123.000000", format!("{}", Fixed6D32::from_f32(123.)));
        assert_eq!("12.300000", format!("{}", Fixed6D32::from_f32(12.3)));
        assert_eq!("1.230000", format!("{}", Fixed6D32::from_f32(1.23)));
        assert_eq!("0.123000", format!("{}", Fixed6D32::from_f32(0.123)));
        assert_eq!("0.012300", format!("{}", Fixed6D32::from_f32(0.0123)));
        assert_eq!("0.001230", format!("{}", Fixed6D32::from_f32(0.00123)));
        assert_eq!("0.000123", format!("{}", Fixed6D32::from_f32(0.000123)));
        assert_eq!("0.000012", format!("{}", Fixed6D32::from_f32(0.0000123)));
        assert_eq!("0.000001", format!("{}", Fixed6D32::from_f32(0.00000123)));
        assert_eq!("0.000000", format!("{}", Fixed6D32::from_f32(0.000000123)));
    }
}
