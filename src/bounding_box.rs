use std::fmt;
use crate::points::{self, ColorPoint};

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
        !(self.ur < other.lr || other.ur < self.lr) &&
        !(self.ug < other.lg || other.ug < self.lg) &&
        !(self.ub < other.lb || other.ub < self.lb)
    }

    pub fn contains(&self, other: &BoundingBox) -> bool {
        other.ur <= self.ur && other.lr >= self.lr &&
        other.ug <= self.ug && other.lg >= self.lg &&
        other.ub <= self.ub && other.lb >= self.lb
    }

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
        let bb = BoundingBox::new(0, 0, 0, 0, 0, 0);
        bb.set_around(center, radius);
        bb
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