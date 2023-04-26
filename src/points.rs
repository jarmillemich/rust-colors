use std::fmt::{self, Debug};

// #[derive(Clone, Debug, Hash, Eq, PartialEq)]
// pub struct SpacePoint {
//     pub x: u32,
//     pub y: u32,
// }

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct SpacePoint(pub u32);


impl SpacePoint {
    pub fn zero() -> SpacePoint { SpacePoint(0) }
    
    pub fn new(x: u32, y: u32) -> SpacePoint {
        SpacePoint(y << 12 | x)
    }

    pub fn xy(&self) -> (u32, u32) {
        let x = self.0 & 4095;
        let y = (self.0 >> 12) & 4095;
        (x, y)
    }

    pub fn get_neighbors(&self, ret: &mut Vec<SpacePoint>)  {
        // TODO we should keep this around and reuse it instead of alloc'ing it
        let (x, y) = self.xy();
        ret.clear();

        if x > 0    { ret.push(SpacePoint::new(x - 1, y    )); }
        if x < 4095 { ret.push(SpacePoint::new(x + 1, y    )); }
        if y > 0    { ret.push(SpacePoint::new(x,     y - 1)); }
        if y < 4095 { ret.push(SpacePoint::new(x,     y + 1)); }

        //ret
    }

    pub fn offset(&self) -> usize {
        let (x, y) = self.xy();
        usize::try_from(y << 12 | x).expect("Should not index a point beyond 2^24")
    }
}

impl fmt::Display for SpacePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (x, y) = self.xy();
        write!(f, "Space<{},{}>", x, y)
    }
}

#[derive(Clone, Copy, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct ColorPoint {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    //channels: i32x4,
}

impl Default for ColorPoint {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

impl Debug for ColorPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Color<{}, {}, {}>", self.r, self.g, self.b)
    }
}

impl ColorPoint {
    pub fn new(r: u8, g: u8, b: u8) -> ColorPoint {

        ColorPoint {
            r,
            g,
            b,
            //channels: i32x4::from_array([r as i32, g as i32, b as i32, 0]),
        }

        
    }

    /// The distance to another color, squared
    #[inline(never)]
    pub fn distance_to(&self, other: &ColorPoint) -> i32 {
        // let delta = self.channels.sub(other.channels);
        // let delta_squared = delta.mul(delta);
        // delta_squared.reduce_sum()

        let dr = self.r as i32 - other.r as i32;
        let dg = self.g as i32 - other.g as i32;
        let db = self.b as i32 - other.b as i32;
        dr * dr + dg * dg + db * db
    }

    pub fn offset(&self) -> usize {
        usize::from(self.r) |
        (usize::from(self.g)<<8) | 
        (usize::from(self.b)<<16)
    }
}

impl fmt::Display for ColorPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Color<{},{},{}>", self.r, self.g, self.b)
    }
}

#[derive(Clone, Copy, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct Point {
    space: SpacePoint,
    color: ColorPoint,
}

impl Point {
    pub fn new(space: SpacePoint, color: ColorPoint) -> Point {
        Point {
            space,
            color,
        }
    }

    pub fn space(&self) -> &SpacePoint {
        &self.space
    }

    pub fn color(&self) -> &ColorPoint {
        &self.color
    }
}

// #[derive(Hash, Eq, PartialEq, PartialOrd)]
// pub struct Point {
//     pub space: usize,//Arc<SpacePoint>,
//     pub color: ColorPoint,
//     //pub idx: i32,
// }

// impl Ord for Point {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         match self.space.cmp(&other.space) {
//             std::cmp::Ordering::Equal => self.color.cmp(&other.color),
//             std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
//             std::cmp::Ordering::Less => std::cmp::Ordering::Less
//         }
//     }
// }


impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (x, y) = self.space().xy();
        let color = self.color();

        write!(f, "Point<{},{} # {},{},{}>",
            x, y,
            color.r, color.g, color.b
        )
    }
}

impl Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (x, y) = self.space().xy();
        let color = self.color();

        write!(f, "Point<{},{} # {},{},{}>",
            x, y,
            color.r, color.g, color.b
        )
    }
}