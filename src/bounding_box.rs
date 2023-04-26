use std::fmt;
use crate::points::{self, ColorPoint};

#[derive(Debug)]
pub struct BoundingBox {
  // Lower RGB bounds
  pub lr: i32,
  pub lg: i32,
  pub lb: i32,

  // Upper RGB bounds
  pub ur: i32,
  pub ug: i32,
  pub ub: i32,
}

impl BoundingBox {
    pub fn new(lr: i32, lg: i32, lb: i32, ur: i32, ug: i32, ub: i32) -> BoundingBox {
        BoundingBox { lr, lg, lb, ur, ug, ub }
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        !(self.ur < other.lr || other.ur < self.lr ||
          self.ug < other.lg || other.ug < self.lg ||
          self.ub < other.lb || other.ub < self.lb)
    }

    pub fn contains(&self, other: &BoundingBox) -> bool {
        other.ur <= self.ur && other.lr >= self.lr &&
        other.ug <= self.ug && other.lg >= self.lg &&
        other.ub <= self.ub && other.lb >= self.lb
    }

    #[inline(never)]
    pub fn contains_color(&self, color: &ColorPoint) -> bool {
        i32::from(color.r) >= self.lr && i32::from(color.r) <= self.ur &&
        i32::from(color.g) >= self.lg && i32::from(color.g) <= self.ug &&
        i32::from(color.b) >= self.lb && i32::from(color.b) <= self.ub
    }

    #[inline(never)]
    pub fn set_around(&mut self, center: &ColorPoint, radius: i32) {
        assert!(radius > 0, "Tried to set_around with a non-positive radius {radius}");
        
        self.lr = i32::from(center.r) - radius;
        self.ur = i32::from(center.r) + radius;
        self.lg = i32::from(center.g) - radius;
        self.ug = i32::from(center.g) + radius;
        self.lb = i32::from(center.b) - radius;
        self.ub = i32::from(center.b) + radius;
    }

    /// Constructs a BoundingBox around the given center with the given radius
    pub fn from_around(center: &points::ColorPoint, radius: i32) -> BoundingBox {
        let mut bb = BoundingBox::new(0, 0, 0, 0, 0, 0);
        bb.set_around(center, radius);
        bb
    }

    /// Constructs a child of this bounding box for some octree index
    pub fn sub_for_idx(&self, index: usize, radius: i32) -> BoundingBox {
        // Subdivision packing is RGB ---, --+, -+-, -++, +--, +-+, ++-, +++
        assert!(index < 8, "Tried to get sub-bounding box for index {} (must be < 8)", index);
        assert!(radius > 0, "Tried to get sub-bounding box with a non-positive radius {radius}");

        let rp = index & 0b100 != 0;
        let gp = index & 0b010 != 0;
        let bp = index & 0b001 != 0;


        let lr = if  rp { self.lr + radius } else { self.lr };
        let ur = if !rp { self.ur - radius } else { self.ur };
        let lg = if  gp { self.lg + radius } else { self.lg };
        let ug = if !gp { self.ug - radius } else { self.ug };
        let lb = if  bp { self.lb + radius } else { self.lb };
        let ub = if !bp { self.ub - radius } else { self.ub };
        BoundingBox { lr, lg, lb, ur, ug, ub }
    }
}

impl fmt::Display for BoundingBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bounds< R ∈ [{}, {}] G ∈ [{}, {}] B ∈ [{}, {}] >",
            self.lr, self.ur,
            self.lg, self.ug,
            self.lb, self.ub,
        )
    }
}

impl PartialEq for BoundingBox {
    fn eq(&self, other: &Self) -> bool {
        self.lr == other.lr &&
        self.lg == other.lg &&
        self.lb == other.lb &&
        self.ur == other.ur &&
        self.ug == other.ug &&
        self.ub == other.ub
    }
}