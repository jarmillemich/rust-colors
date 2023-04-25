use std::{borrow::Borrow, sync::{Weak, Arc}, ops::Deref};

use crate::{points::{ColorPoint, Point, SpacePoint}, bounding_box::BoundingBox, crashmap::{CrashMap}, nn_search_3d::NnSearch3d};
use parking_lot::RwLock;

//type OctreeLink = RwLock<Octree>;
type ParentLink = Option<Weak<Octree>>;
type ChildLink = RwLock<Option<Arc<Octree>>>;
//type PointBucket = RwLock<Vec<Point>>;
struct PointBucket(RwLock<Vec<Point>>);

impl Deref for PointBucket {
  type Target = RwLock<Vec<Point>>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

pub struct Octree {
  depth: u8,
  parent: ParentLink,
  children: [ChildLink; 8],
  bounds: BoundingBox,
  //point_lookup: Arc<CrashMap<usize, RwLock<Vec<Arc<Point>>>>>,
  //points: Arc<CrashSet<Arc<Point>>>,
  points: Arc<CrashMap<SpacePoint, PointBucket>>,
  coord: usize,
  ptr: RwLock<Weak<Octree>>,
}


struct Search {
  candidate: Point,
  source: ColorPoint,
  best_distance_sq: i32,
  bounds: BoundingBox,
}

static QUAD_TUNING: usize = 8;
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
      //point_lookup: Arc::new(CrashMap::with_capacity(1024)),
      //points: Arc::new(CrashSet::with_capacity(1024)),
      points: Arc::new(CrashMap::with_capacity(256 >> (3 * depth))), // Heuristic
      coord,
      ptr: RwLock::new(Weak::new()),
    });

    *ret.ptr.write() = Arc::downgrade(&ret);

    ret
  }

  //pub fn diameter(&self) -> u8 { 256 >> self.depth }
  pub fn radius(&self) -> i32 { 128 >> self.depth }

  

  // Like remove but we already have all the color/space info
  fn remove_spec(&self, point: Point) {
    // Try to remove, if we didn't have it then we're done

    //println!("  Remove spec {} {} at {} with {} in {}", point.space, point.color.offset(), self.depth, self.len(), self.bounds);
    
    //println!("    Removed {point} at {}", self.depth);
    if self.depth > 0 { // NB we already removed it from the root...
      // Note: this is very fine because we might have already removed this space point
      self.points.remove(point.space());
    }

    // Remove from appropriate child
    if let Some(child) = self.get_child(&point.color()) {
      child.remove_spec(point)
    }
  }

  fn get_or_create_child(&self, color: &ColorPoint) -> Arc<Octree> {
    assert!(self.depth <= TREE_TUNING_DEPTH, "Should not create a child past the tuning depth");
    
    let caddr = self.addr(color);
    let radius = self.radius();

    let mut child = self.children[caddr].write();

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
          Some(Weak::clone(&self.ptr.read())),
          self.depth + 1,
          self.coord | caddr << (18 - 3 * self.depth),
          BoundingBox::new(clr, clg, clb, cur, cug, cub),
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
      .read()
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

  fn nearest_in_self(&self, color: &ColorPoint) -> Option<Point> {
    //println!("Selfish for {color} at {} with {} and {}", self.bounds, self.points.len(), self.points.get_capacity());

    let mut best_dist = i32::MAX;
    let mut best = None;
    let mut what = 0;

    self.points.foreach_lockfree(
      #[inline(never)]
      |(_, points)| {
      //println!("    bucket");
      for p in points.read().iter() {
        //println!("    point");
        let dist = p.color().distance_to(color);
        if dist < best_dist {
          best_dist = dist;
          best = Some(*p);
          what += 1;
        }
      }
    });

    // if what > 10 {
    //   println!("Did {what}");
    // }

    match best {
      Some(p) => Some(p),
      None => None
    }
  }

  fn nn_search_up(&self, mut search: Search, from: Arc<Octree>) -> Option<Search> {
    assert!(search.bounds.intersects(&self.bounds), "Searching up a non-intersecting tree");

    // Search all the children we didn't come from
    for child in &self.children {
      match child.read().as_ref() {
        None => {},
        Some(c) => {
          let to_search = Arc::clone(c);
          if !Arc::ptr_eq(&to_search, &from) {
            // Search down other children
            search = to_search.nn_search_down(search)?;
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
        .nn_search_up(search, self.ptr.read().upgrade().expect("should have self"))?;
    }

    Some(search)
  }

  fn nn_search_down(&self, mut search: Search) -> Option<Search> {
    // Skip us if not in search space
    if !search.bounds.intersects(&self.bounds) { return Some(search); }

    // We have no points to search
    if self.points.is_empty() { return Some(search); }

    if self.points.len() <= QUAD_TUNING {
      // We have few enough points, search here
      let our_nearest = self.nearest_in_self(&search.source)?;
      let nearest_dist = search.source.distance_to(&our_nearest.color());

      if nearest_dist < search.best_distance_sq {
        // New candidate!
        search.candidate = our_nearest;
        search.best_distance_sq = nearest_dist;
        search.bounds.set_around(&search.source, f64::from(nearest_dist).sqrt().floor() as i32);
      }
    } else if self.depth < TREE_TUNING_DEPTH {
      // Keep going down!
      
      for child in &self.children {
        if let Some(c) = child.read().as_ref() {
          search = c.nn_search_down(search)?;
        }
      }
    }

    Some(search)
  }

  
}

impl NnSearch3d for Octree {
  fn len(&self) -> usize {
    self.points.len()
  }

  fn is_empty(&self) -> bool {
    self.points.is_empty()
  }

  fn has(&self, pt: SpacePoint) -> bool {
    self.points.contains_key(pt)
  }

  fn has_point(&self, pt: &Point) -> bool {
    self.points.get(&pt.space(), |colors| {
      colors.read().contains(pt)
    }).unwrap_or(false)
  }

  fn add(&self, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
    //println!("    Add {point} at {}", self.depth);
    
    if self.depth < TREE_TUNING_DEPTH {
      // Head downwards
      // NB Probably it is important for thread-ness that we add to our children first?
      // But what if we add to child, search find, start removing, and we haven't gotten back to the root yet?
      // TODO add another bitvec for "available" to do the search retry things?
      self.get_or_create_child(&point.color()).add(point, spare_vectors);
    }

    //println!("Adding {} {} at {} with {} in {}", &point.space, point.color.offset(), self.depth, self.len(), self.bounds);

    // Add the point here
    self.points.get_or_insert(
      point.space(),
      // Grab us some space if we didn't have this list yet
      #[inline(never)]
      || {
        //point_pool.pull()
        PointBucket(RwLock::new(Vec::with_capacity(4)))
      },
      #[inline(never)]
      move |p| {
        p.write().push(point);
      }
    );
      
    assert!(self.len() > 0);
    
  }

  fn add_sync(&mut self, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
    self.add(point, spare_vectors);
  }

  fn remove(&self, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
    if !self.has_point(&point) {
      panic!("Removing non-existent point {point}");
    }

    //println!("Remove {} at {} with {}", point.space, self.depth, self.len());

    // NB we are removing by the spatial component here, so get all the actual points with this
    // Grab all our Rcs to remove
    let pts_maybe = self.points.remove(point.space());

    //println!("    Removing {} instances of color {}", pts.borrow().len(), &point.color);
    if let Some(pts) = pts_maybe {
      //assert!(self.has_point(point), "Removing from list but not in lookup");

      for rc in pts.read().iter() {
        // Remove from self
        self.remove_spec(*rc);
      }
    }

    //assert!(!self.point_lookup.contains_key(&point.space), "Tried to remove a point but still present");
    //assert!(!self.points.contains_key(point), "Tried to remove a point but still present");

    //pts_maybe
  }

  fn remove_sync(&mut self, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
    self.remove(point, spare_vectors);
  }

  fn find_nearest(&self, color: &ColorPoint) -> Option<Point> {
    let child = self.get_child(color);

    if self.points.is_empty() {
      //panic!("Tried to find nearest but no points at depth {0}", self.depth);
      // Probably this occurs because of threading...
      return None;
    }

    let have_search_child = match child {
      Some(ref c) => !c.as_ref().points.is_empty(),
      None => false
    };
    
    //println!("Search {color} at {} with {}", self.depth, self.len());

    if self.points.len() <= QUAD_TUNING || !have_search_child {
      // If we are small or we have no children, search here
      let ret = self.nearest_in_self(color)?;

      let distance = ret.color().distance_to(color);
      let radius_sq = self.radius() * self.radius();

      if self.depth > 0 && distance > radius_sq {
        // The distance to the nearest candidate is bigger than our own radius
        // Therefore, we need to search our neighbors too
        let search_radius = f64::from(distance).sqrt().floor() as i32;

        let mut search = Search {
          candidate: ret,
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
          .nn_search_up(search, self.ptr.read().upgrade().expect("should have self"))?;

        return Some(search.candidate);
      }

      Some(ret)
    } else {
      child?.as_ref().borrow().find_nearest(color)
    }
  }
}