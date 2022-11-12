use std::{fmt, rc::Rc};

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct SpacePoint {
    pub x: u32,
    pub y: u32,
}

fn space_offset(x: u32, y: u32) -> usize {
    (y << 12 | x) as usize
}

impl SpacePoint {
    pub fn zero() -> SpacePoint { SpacePoint::new(0, 0) }
    
    pub fn new(x: u32, y: u32) -> SpacePoint {
        SpacePoint {
            x,
            y,
            //written: RefCell::new(false),
        }
    }

    pub fn get_neighbors(&self) -> Vec<usize> {
        // Yikes
        let mut ret = Vec::with_capacity(4);

        if self.x > 0    { ret.push(space_offset(self.x - 1, self.y)); }
        if self.x < 4095 { ret.push(space_offset(self.x + 1, self.y)); }
        if self.y > 0    { ret.push(space_offset(self.x, self.y - 1)); }
        if self.y < 4095 { ret.push(space_offset(self.x, self.y + 1)); }

        ret
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

impl Default for ColorPoint {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorPoint {
    pub fn new() -> ColorPoint {
        ColorPoint { r: 0, g: 0, b: 0 }
    }

    pub fn distance_to(&self, other: &Rc<ColorPoint>) -> i32 {
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
    pub space: usize,//Rc<SpacePoint>,
    pub color: Rc<ColorPoint>,
    //pub idx: i32,
}


impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point<{},{} # {},{},{}>",
            self.space, self.space,
            self.color.r, self.color.g, self.color.b
        )
    }
}