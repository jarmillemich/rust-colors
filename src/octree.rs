use std::{borrow::Borrow, sync::{Weak, Arc, RwLock}};
use fnv::{FnvHashMap, FnvHashSet};
use chashmap::CHashMap;

use crate::{points::{ColorPoint, Point}, bounding_box::BoundingBox};

//type OctreeLink = RwLock<Octree>;
type ParentLink = Option<Weak<Octree>>;
type ChildLink = RwLock<Option<Arc<Octree>>>;

pub struct Octree {
  depth: u8,
  parent: ParentLink,
  children: [ChildLink; 8],
  bounds: BoundingBox,
  point_lookup: RwLock<FnvHashMap<usize, RwLock<Vec<Arc<Point>>>>>,
  points: RwLock<FnvHashSet<Arc<Point>>>,
  coord: usize,
  ptr: RwLock<Weak<Octree>>,
}

struct Search {
  canidate: Arc<Point>,
  source: ColorPoint,
  best_distance_sq: i32,
  bounds: BoundingBox,
}

static QUAD_TUNING: usize = 64;
static TREE_TUNING_DEPTH: u8 = 3;

impl Octree {
  pub fn new(
    parent: ParentLink, depth: u8, coord: usize,
    bounds: BoundingBox
  ) -> Arc<Octree> {
    let ret = Arc::new(Octree {
      depth,
      // TODO why can't we use the quick array literal here?
      //      Box isn't copyable?
      children: array_init::array_init(|_| RwLock::new(None)),
      parent,
      bounds,
      point_lookup: RwLock::new(FnvHashMap::default()),
      points: RwLock::new(FnvHashSet::default()),
      coord,
      ptr: RwLock::new(Weak::new()),
    });

    *ret.ptr.write().unwrap() = Arc::downgrade(&ret);

    ret
  }

  //pub fn diameter(&self) -> u8 { 256 >> self.depth }
  pub fn radius(&self) -> i32 { 128 >> self.depth }

  pub fn has(&self, pt: usize) -> bool {
    self.point_lookup.read().unwrap().contains_key(&pt)
  }

  pub fn has_point(&self, pt: &Point) -> bool {
    self.points.read().unwrap().contains(pt)
  }

  pub fn add(&self, point: Arc<Point>) {
    //println!("    Add {point} at {}", self.depth);
    
    if self.depth < TREE_TUNING_DEPTH {
      // Head downwards
      // NB Probably it is important for thread-ness that we add to our children first?
      // But what if we add to child, search find, start removing, and we haven't gotten back to the root yet?
      // TODO add another bitvec for "available" to do the search retry things?
      self.get_or_create_child(&point.color).add(Arc::clone(&point));
    }

    // Add to the lookup helper on this node
    self.point_lookup
      .write().unwrap()
      .entry(point.space)
      // Pre-allocating does not in fact save too much time, unless there's a better strategy?
      //.or_insert_with(|| RwLock::new(Vec::with_capacity(16384 >> (3 * self.depth))))
      .or_insert_with(|| RwLock::new(Vec::new()))
      .write().unwrap()
      .push(Arc::clone(&point));

    //println!("Added {} {} at {} with {} in {}", &point.space, point.color.offset(), self.depth, self.len(), self.bounds);

    // Add to this node
    self.points.write().unwrap().insert(point);

    
  }

  pub fn remove(&self, point: &Arc<Point>) {
    if !self.points.read().unwrap().contains(point) {
      panic!("Removing non-existent point {point}");
    }

    //println!("Remove {} at {} with {}", point.space, self.depth, self.len());

    // NB we are removing by the spatial component here, so get all the actual points with this
    // Grab all our Rcs to remove
    let pts_maybe = {
      let mut hm = self.point_lookup.write().unwrap();

      hm.remove(&point.space)
    };

    //println!("    Removing {} instances of color {}", pts.borrow().len(), &point.color);
    if let Some(pts) = pts_maybe {
      let rm = pts.into_inner().unwrap();
      assert!(rm.contains(point), "Removing from list but not in lookup");

      for rc in rm {
        // Remove from self
        self.remove_spec(&rc);
      }
    }

    assert!(!self.point_lookup.read().unwrap().contains_key(&point.space), "Tried to remove a point but still present");
    assert!(!self.points.read().unwrap().contains(point), "Tried to remove a point but still present");

    //pts_maybe
  }

  // Like remove but we already have all the color/space info
  fn remove_spec(&self, point: &Arc<Point>) {
    let mut point_write = self.points.write().unwrap();
    let mut lookup_write = self.point_lookup.write().unwrap();

    // Try to remove, if we didn't have it then we're done
    if !point_write.remove(point) {
      return;
    }

    //println!("  Remove spec {} {} at {} with {} in {}", point.space, point.color.offset(), self.depth, self.len(), self.bounds);
    
    //println!("    Removed {point} at {}", self.depth);
    if self.depth > 0 { // NB we already removed it from the root...
      // Note: this is very fine because we might have already removed this space point
      let removed = lookup_write.remove(&point.space);
      // match removed {
      //   None => panic!("Removed from points but not in point_lookup at {} with {}", self.depth, self.len()),
      //   _ => {}
      // };
    }

    // Remove from appropriate child
    if let Some(child) = self.get_child(&point.color) {
      child.remove_spec(point)
    }
  }

  // Debugging some things..
  pub fn is_very_removed(&self, pt: &Arc<Point>) -> bool {
    if self.has(pt.space) { return false; }
    if self.has_point(pt) { return false; }

    if let Some(child) = self.get_child(&pt.color) {
      return child.is_very_removed(pt);
    } else {
      return true;
    }
  }

  fn get_or_create_child(&self, color: &ColorPoint) -> Arc<Octree> {
    assert!(self.depth <= TREE_TUNING_DEPTH, "Should not create a child past the tuning depth");
    
    let caddr = self.addr(color);
    let radius = self.radius();

    let mut child = self.children[caddr].write().unwrap();

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
          Some(Weak::clone(&self.ptr.read().unwrap())),
          self.depth + 1,
          self.coord | caddr << (18 - 3 * self.depth),
          BoundingBox::new(clr, clg, clb, cur, cug, cub)
        );

        Some(child)
      }
    };
    
    //self.get_or_create_child_inner(color, &mut self.children[caddr]);
    let thing = child.as_ref().expect("Just created a child, it should exist");
    Arc::clone(thing)
  }

  fn get_child(&self, color: &ColorPoint) -> Option<Arc<Octree>> {
    self.children[self.addr(color)]
      .read().unwrap()
      .as_ref()
      .map(Arc::clone)
  }

  fn addr(&self, color: &ColorPoint) -> usize {
    let mask = self.radius();
    let over = 7 - self.depth;

    let raddr = (color.r as i32 & mask) >> over;
    let gaddr = (color.g as i32 & mask) >> over;
    let baddr = (color.b as i32 & mask) >> over;

    (raddr << 2 | gaddr << 1 | baddr) as usize
  }

  pub fn find_nearest(&self, color: &ColorPoint) -> Option<Arc<Point>> {
    let child = self.get_child(color);

    if self.points.read().unwrap().is_empty() {
      panic!("Tried to find nearest but no points at depth {0}", self.depth);
    }

    let have_search_child = match child {
      Some(ref c) => !c.as_ref().points.read().unwrap().is_empty(),
      None => false
    };

    if self.points.read().unwrap().len() <= QUAD_TUNING || !have_search_child {
      // If we are small or we have no children, search here
      let ret = self.nearest_in_self(color);

      let distance = ret.color.distance_to(color);
      let radius_sq = self.radius() * self.radius();

      if self.depth > 0 && distance > radius_sq {
        // The distance to the nearest candidate is bigger than our own radius
        // Therefore, we need to search our neighbors too
        let search_radius = f64::from(distance).sqrt().floor() as i32;

        let mut search = Search {
          canidate: ret,
          source: color.clone(),
          best_distance_sq: distance,
          bounds: BoundingBox::from_around(color, search_radius)
        };

        search = self.parent.as_ref()
          .expect("depth > 0 should have a parent")
          .upgrade()
          .expect("depth > 0 should have a non-deleted parent")
          .as_ref()
          .borrow()
          .nn_search_up(search, self.ptr.read().unwrap().upgrade().expect("should have self"));

        return Some(Arc::clone(&search.canidate));
      }

      Some(ret)
    } else {
      child?.as_ref().borrow().find_nearest(color)
    }
  }

  fn nearest_in_self(&self, color: &ColorPoint) -> Arc<Point> {
    let points = self.points.read().unwrap();
    let result = points.iter()
      .map(|p| (p, p.color.distance_to(color)))
      .min_by(|a, b| a.1.cmp(&b.1));

    Arc::clone(result.expect("Should have had at least one point for nearest_in_self").0)
  }

  fn nn_search_up(&self, mut search: Search, from: Arc<Octree>) -> Search {
    assert!(search.bounds.intersects(&self.bounds), "Searching up a non-intersecting tree");

    // Search all the children we didn't come from
    for child in &self.children {
      match child.read().unwrap().as_ref() {
        None => {},
        Some(c) => {
          let to_search = Arc::clone(c);
          if !Arc::ptr_eq(&to_search, &from) {
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
        .nn_search_up(search, self.ptr.read().unwrap().upgrade().expect("should have self"));
    }

    search
  }

  fn nn_search_down(&self, mut search: Search) -> Search {
    // Skip us if not in search space
    if !search.bounds.intersects(&self.bounds) { return search; }

    // We have no points to search
    if self.points.read().unwrap().is_empty() { return search; }

    if self.points.read().unwrap().len() <= QUAD_TUNING {
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
        if let Some(c) = child.read().unwrap().as_ref() {
          search = c.nn_search_down(search);
        }
      }
    }

    search
  }

  pub fn len(&self) -> usize {
    self.points.read().unwrap().len()
  }

  pub fn is_empty(&self) -> bool {
    self.points.read().unwrap().is_empty()
  }
}