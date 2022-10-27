use std::{collections::HashSet, cell::RefCell,rc::{Rc,Weak}, borrow::{BorrowMut, Borrow}, ops::DerefMut};

use crate::{points::{ColorPoint, Point}, bounding_box::BoundingBox};

//type OctreeLink = RefCell<Octree>;
type ParentLink<'a> = Option<Weak<Octree<'a>>>;
type ChildLink<'a> = RefCell<Option<Rc<Octree<'a>>>>;

pub struct Octree<'a> {
  depth: u8,
  parent: ParentLink<'a>,
  children: [ChildLink<'a>; 8],
  bounds: BoundingBox,
  points: RefCell<HashSet<Rc<Point<'a>>>>,
  coord: usize,
  ptr: RefCell<Weak<Octree<'a>>>,
}

struct Search<'a> {
  canidate: Rc<Point<'a>>,
  source: Rc<ColorPoint>,
  best_distance_sq: i32,
  bounds: BoundingBox,
}

static QUAD_TUNING: usize = 64;
static TREE_TUNING_DEPTH: u8 = 4;

impl<'a> Octree<'a> {
  pub fn new(
    parent: ParentLink<'a>, depth: u8, coord: usize,
    bounds: BoundingBox
  ) -> Rc<Octree<'a>> {
    let ret = Rc::new(Octree {
      depth,
      // TODO why can't we use the quick array literal here?
      //      Box isn't copyable?
      children: array_init::array_init(|_| RefCell::new(None)),
      parent,
      bounds,
      points: RefCell::new(HashSet::new()),
      coord,
      ptr: RefCell::new(Weak::new()),
    });

    *ret.ptr.borrow_mut() = Rc::downgrade(&ret);

    ret
  }

  //pub fn diameter(&self) -> u8 { 256 >> self.depth }
  pub fn radius(&self) -> u8 { 128 >> self.depth }

  pub fn add(&self, point: Rc<Point>) {
    
    
    if self.depth < TREE_TUNING_DEPTH {
      // Head downwards
      let child = self.get_or_create_child(&point.color);
      let child = child.borrow_mut();
      child.add(Rc::clone(&point));
    }

    // Add to this node
    let someClone = Rc::clone(&point);
    self.points.borrow_mut().insert(someClone);
  }

  pub fn remove(&self, point: &Rc<Point<'a>>) {
    if !self.points.borrow().contains(point) {
      panic!("Removing non-existent point {point}");
    }

    // Remove from children
    if self.depth < TREE_TUNING_DEPTH {
      match self.children[self.addr(&point.color)].borrow().as_ref() {
        Some(child) => child.remove(point),
        None => panic!("Tried to remove from a non-existant child node")
      }
    }

    // Remove from self
    self.points.borrow_mut().remove(point);
  }

  fn get_or_create_child(&self, color: &Rc<ColorPoint>) -> RefCell<Rc<Octree<'a>>> {
    assert!(self.depth < TREE_TUNING_DEPTH, "Should not create a child past the tuning depth");
    
    let caddr = self.addr(color);
    let radius = i32::from(self.radius());

    let mut child = self.children[caddr].borrow_mut();

    *child = match child.as_ref() {
      Some(c) => Some(c.to_owned()),
      None => {
        // Calculate new bounds
        let bounds = &self.bounds;
        let clr = if i32::from(color.r) > bounds.lr + radius { bounds.lr + radius } else { bounds.lr };
        let cur = if i32::from(color.r) < bounds.ur - radius { bounds.ur - radius } else { bounds.ur };
        let clg = if i32::from(color.g) > bounds.lg + radius { bounds.lg + radius } else { bounds.lg };
        let cug = if i32::from(color.g) < bounds.ug - radius { bounds.ug - radius } else { bounds.ug };
        let clb = if i32::from(color.b) > bounds.lb + radius { bounds.lb + radius } else { bounds.lb };
        let cub = if i32::from(color.b) < bounds.ub - radius { bounds.ub - radius } else { bounds.ub };

        let child = Octree::new(
          Some(Weak::clone(&self.ptr.borrow())),
          self.depth + 1,
          self.coord | caddr << (18 - 3 * self.depth),
          BoundingBox::new(clr, clg, clb, cur, cug, cub)
        );

        Some(child)
      }
    };
    
    //self.get_or_create_child_inner(color, &mut self.children[caddr]);
    let thing = child.as_ref().expect("Just created a child, it should exist");
    thing.as_ref().borrow_mut().get_or_create_child(color)
  }

  fn get_child(&self, color: &Rc<ColorPoint>) -> Option<Rc<Octree<'a>>> {
    match self.children[self.addr(color)].borrow().as_ref() {
      Some(c) => Some(Rc::clone(&c)),
      None => None
    }
  }
  
  // fn get_child_mut(&'a mut self, color: &'a ColorPoint) -> Option<&mut Box<Octree>> {
  //   self.children[self.addr(color)]?.borrow_mut()
  // }

  fn addr(&self, color: &Rc<ColorPoint>) -> usize {
    let mask = self.radius();
    let over = 7 - self.depth;

    let raddr = (color.r & mask) >> over;
    let gaddr = (color.g & mask) >> over;
    let baddr = (color.b & mask) >> over;

    usize::from(raddr << 2 | gaddr << 1 | baddr)
  }

  pub fn find_nearest(&self, color: &Rc<ColorPoint>) -> Option<Rc<Point<'a>>> {
    let child = self.get_child(color);

    if self.points.borrow().is_empty() {
      panic!("Tried to find nearest but no points at depth {0}", self.depth);
    }

    let have_search_child = match child {
      Some(ref c) => !c.as_ref().points.borrow().is_empty(),
      None => false
    };

    if self.points.borrow().len() <= QUAD_TUNING || !have_search_child {
      // If we are small or we have no children, search here
      let ret = self.nearest_in_self(color);

      let distance = ret.color.distance_to(color);
      let radius_sq = self.radius() * self.radius();

      if self.depth > 0 && distance > radius_sq as i32 {
        // The distance to the nearest candidate is bigger than our own radius
        // Therefore, we need to search our neighbors too
        let search_radius = f64::from(distance).sqrt().floor() as i32;

        let mut search = Search {
          canidate: ret,
          source: Rc::clone(color),
          best_distance_sq: distance,
          bounds: BoundingBox::from_around(color, search_radius)
        };

        search = self.parent.as_ref()
          .expect("depth > 0 should have a parent")
          .upgrade()
          .expect("depth > 0 should have a non-deleted parent")
          .as_ref()
          .borrow()
          .nn_search_up(search, self.ptr.borrow().upgrade().expect("should have self"));

        return Some(Rc::clone(&search.canidate));
      }

      return Some(ret);
    } else {
      return child?.as_ref().borrow().find_nearest(color);
    }
  }

  fn nearest_in_self(&self, color: &Rc<ColorPoint>) -> Rc<Point<'a>> {
    let points = self.points.borrow();
    let result = points.iter()
      .map(|p| (p, p.color.distance_to(color)))
      .min_by(|a, b| a.1.cmp(&b.1));

    Rc::clone(result.expect("Should have had at least one point for nearest_in_self").0)
  }

  fn nn_search_up(&self, mut search: Search<'a>, from: Rc<Octree<'a>>) -> Search<'a> {
    assert!(search.bounds.intersects(&self.bounds), "Searching up a non-intersecting tree");

    // Search all the children we didn't come from
    for child in &self.children {
      match child.borrow().as_ref() {
        None => {},
        Some(c) => {
          let to_search = Rc::clone(c);
          if !Rc::ptr_eq(&to_search, &from) {
            // Search down other children
            search = to_search.nn_search_down(search);
          }
        }
      }
    }

    // If the search space is still outside us and we're not root
    if self.depth > 0 && !self.bounds.contains(&search.bounds) {
      search = self.parent.as_ref()
        .expect("depth > 0 should have a parent")
        .upgrade()
        .expect("depth > 0 parent should not have been deleted")
        .as_ref()
        .borrow()
        .nn_search_up(search, self.ptr.borrow().upgrade().expect("should have self"));
    }

    search
  }

  fn nn_search_down(&self, mut search: Search<'a>) -> Search<'a> {
    // Skip us if not in search space
    if !search.bounds.intersects(&self.bounds) { return search; }

    // We have no points to search
    if self.points.borrow().is_empty() { return search; }

    if self.points.borrow().len() <= QUAD_TUNING {
      // We have few enough points, search here
      let our_nearest = self.nearest_in_self(&search.source);
      let nearest_dist = search.source.distance_to(&our_nearest.color);

      if nearest_dist < search.best_distance_sq {
        // New candidate!
        search.canidate = our_nearest;
        search.best_distance_sq = nearest_dist;
        search.bounds.set_around(&search.source, f64::from(nearest_dist).sqrt().floor() as i32);
      }
    } else if self.depth < TREE_TUNING_DEPTH {
      // Keep going down!
      for child in &self.children {
        match child.borrow().as_ref() {
          Some(c) => { search = c.nn_search_down(search) },
          None => {}
        };
      }
    }

    search
  }
}