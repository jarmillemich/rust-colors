use crate::points::{Point, ColorPoint, SpacePoint};

pub trait NnSearch3d {
    fn add(&self, point: Point, spare_vectors: &mut Vec<Vec<Point>>);
    fn add_sync(&mut self, point: Point, spare_vectors: &mut Vec<Vec<Point>>);
    fn remove(&self, point: Point, spare_vectors: &mut Vec<Vec<Point>>);
    fn remove_sync(&mut self, point: Point, spare_vectors: &mut Vec<Vec<Point>>);
    fn find_nearest(&self, pt: &ColorPoint) -> Option<Point>;

    fn has(&self, pt: SpacePoint) -> bool;
    fn has_point(&self, pt: &Point) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}