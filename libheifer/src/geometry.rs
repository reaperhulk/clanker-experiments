// SPDX-License-Identifier: LGPL-3.0-or-later
// Fraction and clean-aperture semantics adapted from libheif, Copyright Dirk Farin and contributors.
use crate::context::ContextError;
type Result<T> = std::result::Result<T, ContextError>;
#[derive(Clone, Copy)]
struct Fraction {
    n: i64,
    d: i64,
}
impl Fraction {
    fn external(mut n: i64, mut d: i64) -> Result<Self> {
        if i32::try_from(n).is_err() || i32::try_from(d).is_err() {
            return Err(ContextError::invalid(
                128,
                "Invalid fractional number: Fraction value exceeds the supported range",
            ));
        }
        while !(-65536..=65536).contains(&d) {
            n /= 2;
            d /= 2;
        }
        while d > 1 && !(-65536..=65536).contains(&n) {
            n /= 2;
            d /= 2;
        }
        if d == 0 {
            return Err(ContextError::invalid(
                128,
                "Invalid fractional number: Fraction with zero denominator",
            ));
        }
        Ok(Self { n, d })
    }
    fn arithmetic(mut n: i64, mut d: i64) -> Self {
        while i32::try_from(n).is_err() || i32::try_from(d).is_err() {
            n = (n + if n >= 0 { 1 } else { -1 }) / 2;
            d = (d + if d >= 0 { 1 } else { -1 }) / 2;
        }
        Self { n, d }
    }
    fn add(self, b: Self) -> Self {
        if self.d == b.d {
            Self::arithmetic(self.n + b.n, self.d)
        } else {
            Self::arithmetic(self.n * b.d + b.n * self.d, self.d * b.d)
        }
    }
    fn sub(self, b: Self) -> Self {
        if self.d == b.d {
            Self::arithmetic(self.n - b.n, self.d)
        } else {
            Self::arithmetic(self.n * b.d - b.n * self.d, self.d * b.d)
        }
    }
    fn integer(self, n: i64) -> Self {
        Self::arithmetic(self.n + n * self.d, self.d)
    }
    fn half(self) -> Self {
        Self::arithmetic(self.n, self.d * 2)
    }
    fn trunc(self) -> i64 {
        self.n / self.d
    }
    fn round(self) -> i64 {
        (self.n + self.d / 2) / self.d
    }
}
pub struct CleanAperture {
    width: Fraction,
    height: Fraction,
    horizontal: Fraction,
    vertical: Fraction,
}
impl CleanAperture {
    pub fn parse(p: &[u8]) -> Result<Self> {
        // The pinned reader returns zero for an incomplete scalar, then validates
        // the resulting fractions before reporting its accumulated read error.
        let n = |i| {
            p.get(i..i + 4)
                .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap()))
        };
        Ok(Self {
            width: Fraction::external(n(0).into(), n(4).into())?,
            height: Fraction::external(n(8).into(), n(12).into())?,
            horizontal: Fraction::external(i64::from(n(16) as i32), n(20).into())?,
            vertical: Fraction::external(i64::from(n(24) as i32), n(28).into())?,
        })
    }
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width.round() as u32, self.height.round() as u32)
    }
    pub fn unclamped_crop(&self, width: u32, height: u32) -> Result<(i64, i64, i64, i64)> {
        if width == 0 || height == 0 {
            return Err(ContextError::invalid(
                120,
                "Invalid clean-aperture specification: Clean aperture cannot be applied to an image with zero size",
            ));
        }
        let half_width = Fraction::external(i64::from(width) - 1, 2);
        let half_height = Fraction::external(i64::from(height) - 1, 2);
        let (Ok(half_width), Ok(half_height)) = (half_width, half_height) else {
            return Err(ContextError::invalid(
                120,
                "Invalid clean-aperture specification: Clean aperture cannot be applied to an image larger than 2^31 pixels in any direction",
            ));
        };
        let left = self
            .horizontal
            .add(half_width)
            .sub(self.width.integer(-1).half())
            .trunc();
        let top = self
            .vertical
            .add(half_height)
            .sub(self.height.integer(-1).half())
            .round();
        let right = self.width.integer(-1).integer(left).round();
        let bottom = self.height.integer(-1).integer(top).round();
        Ok((left, right, top, bottom))
    }
    pub fn crop(&self, width: u32, height: u32) -> Result<(u32, u32, u32, u32)> {
        let (left, right, top, bottom) = self.unclamped_crop(width, height)?;
        let (left, top) = (left.max(0), top.max(0));
        // Upstream compares the right/bottom endpoints after unsigned conversion.
        let right = if right < 0 || right >= i64::from(width) {
            i64::from(width) - 1
        } else {
            right
        };
        let bottom = if bottom < 0 || bottom >= i64::from(height) {
            i64::from(height) - 1
        } else {
            bottom
        };
        if left > right || top > bottom {
            return Err(ContextError::invalid(
                120,
                "Invalid clean-aperture specification",
            ));
        }
        Ok((left as u32, right as u32, top as u32, bottom as u32))
    }
}
