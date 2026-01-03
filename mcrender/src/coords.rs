use std::ops::Mul;

/// Generic 2D point, intended for use with numeric types.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
#[mul(forward)]
pub struct Vec2D<T: Copy>(pub T, pub T);

impl<T: Copy + Mul<Output = T>> Mul<T> for Vec2D<T> {
    type Output = Vec2D<T>;

    fn mul(mut self, rhs: T) -> Self::Output {
        self.0 = self.0 * rhs;
        self.1 = self.1 * rhs;
        self
    }
}

/// Wrap Vec2D<T> to give right-to-left ordering and convenience methods useful for Minecraft data operations.
#[derive(
    Clone,
    Copy,
    Default,
    Eq,
    PartialEq,
    Hash,
    derive_more::Debug,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
#[debug(bounds(T: std::fmt::Debug))]
#[debug("PointXZ({:?}, {:?})", _0.0, _0.1)]
#[display(bounds(T: std::fmt::Display))]
#[display("<x={} z={}>", _0.0, _0.1)]
#[mul(forward)]
pub struct PointXZ<T: Copy>(Vec2D<T>);

impl<T: Copy> PointXZ<T> {
    pub const fn new(x: T, z: T) -> Self {
        Self(Vec2D(x, z))
    }

    #[inline]
    pub const fn x(&self) -> T {
        self.0.0
    }

    #[inline]
    pub const fn z(&self) -> T {
        self.0.1
    }
}

impl<T: Copy> From<(T, T)> for PointXZ<T> {
    fn from(value: (T, T)) -> Self {
        Self(value.into())
    }
}

impl<T: Copy> From<PointXZ<T>> for (T, T) {
    fn from(value: PointXZ<T>) -> (T, T) {
        value.0.into()
    }
}

impl<T: Copy + Ord> Ord for PointXZ<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.z(), self.x()).cmp(&(other.z(), other.x()))
    }
}

impl<T: Copy + Ord> PartialOrd for PointXZ<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Generic 3D point, intended for use with numeric types.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
#[mul(forward)]
pub struct Vec3D<T: Copy>(pub T, pub T, pub T);

impl<T: Copy + Mul<Output = T>> Mul<T> for Vec3D<T> {
    type Output = Vec3D<T>;

    fn mul(mut self, rhs: T) -> Self::Output {
        self.0 = self.0 * rhs;
        self.1 = self.1 * rhs;
        self.2 = self.2 * rhs;
        self
    }
}

/// Wrap Vec3D<T> to give right-to-left ordering and convenience methods useful for Minecraft data operations.
#[derive(
    Clone,
    Copy,
    Default,
    Eq,
    PartialEq,
    Hash,
    derive_more::Debug,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
#[debug(bounds(T: std::fmt::Debug))]
#[debug("PointXZY({:?}, {:?}, {:?})", _0.0, _0.1, _0.2)]
#[display(bounds(T: std::fmt::Display))]
#[display("<x={} z={} y={}>", _0.0, _0.1, _0.2)]
#[mul(forward)]
pub struct PointXZY<T: Copy>(Vec3D<T>);

impl<T: Copy> PointXZY<T> {
    pub const fn new(x: T, z: T, y: T) -> Self {
        Self(Vec3D(x, z, y))
    }

    #[inline]
    pub const fn x(&self) -> T {
        self.0.0
    }

    #[inline]
    pub const fn z(&self) -> T {
        self.0.1
    }

    #[inline]
    pub const fn y(&self) -> T {
        self.0.2
    }
}

impl<T: Copy> From<(T, T, T)> for PointXZY<T> {
    fn from(value: (T, T, T)) -> Self {
        Self(value.into())
    }
}

impl<T: Copy> From<PointXZY<T>> for (T, T, T) {
    fn from(value: PointXZY<T>) -> (T, T, T) {
        value.0.into()
    }
}

impl<T: Copy + Ord> Ord for PointXZY<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.y(), self.z(), self.x()).cmp(&(other.y(), other.z(), other.x()))
    }
}

impl<T: Copy + Ord> PartialOrd for PointXZY<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub type CoordsXZ = PointXZ<i32>;
pub type CoordsXZY = PointXZY<i32>;
pub type IndexXZ = PointXZ<u32>;
pub type IndexXZY = PointXZY<u32>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec2d() {
        let a = Vec2D(1, 2);
        assert_eq!(format!("{:?}", a), "Vec2D(1, 2)");
        assert_eq!(Vec2D::from((1, 2)), a);
        assert_eq!((1, 2), a.into());
    }

    #[test]
    fn test_pointxz() {
        let a = PointXZ::new(1, 2);
        assert_eq!(format!("{:?}", a), "PointXZ(1, 2)");
        assert_eq!(format!("{}", a), "<x=1 z=2>");
        assert_eq!(PointXZ::from(Vec2D(1, 2)), a);
        assert_eq!(PointXZ::from((1, 2)), a);
        assert_eq!(Vec2D(1, 2), a.into());
        assert_eq!((1, 2), a.into());
    }

    #[test]
    fn test_pointxzy() {
        let a = PointXZY::new(1, 2, 3);
        assert_eq!(format!("{:?}", a), "PointXZY(1, 2, 3)");
        assert_eq!(format!("{}", a), "<x=1 z=2 y=3>");
        assert_eq!(PointXZY::from(Vec3D(1, 2, 3)), a);
        assert_eq!(PointXZY::from((1, 2, 3)), a);
        assert_eq!(Vec3D(1, 2, 3), a.into());
        assert_eq!((1, 2, 3), a.into());
    }
}
