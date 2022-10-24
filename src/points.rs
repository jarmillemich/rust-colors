use std::fmt;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct SpacePoint {
    pub x: u32,
    pub y: u32,
    //pub written: bool,
}

impl SpacePoint {
    pub fn zero() -> SpacePoint { SpacePoint::new(0, 0) }
    
    pub fn new(x: u32, y: u32) -> SpacePoint {
        SpacePoint {
            x,
            y,
            //written: false,
        }
    }
}

impl fmt::Display for SpacePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Space<{},{}>", self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct ColorPoint {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorPoint {
    pub fn new() -> ColorPoint {
        ColorPoint { r: 0, g: 0, b: 0 }
    }

    pub fn distance_to(&self, other: &ColorPoint) -> i32 {
        let dr: i32 = i32::from(self.r) - i32::from(other.r);
        let dg: i32 = i32::from(self.g) - i32::from(other.g);
        let db: i32 = i32::from(self.b) - i32::from(other.b);

        dr * dr + dg * dg + db * db
    }
}

impl fmt::Display for ColorPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Color<{},{},{}>", self.r, self.g, self.b)
    }
}

#[derive(Hash, Eq, PartialEq)]
pub struct Point {
    pub space: SpacePoint,
    pub color: ColorPoint,
    pub idx: i32,
}


impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point<{},{} # {},{},{}>",
            self.space.x, self.space.y,
            self.color.r, self.color.g, self.color.b
        )
    }
}