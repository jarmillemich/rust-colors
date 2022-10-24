use std::{collections::HashSet,ptr, cell::RefCell,rc::{Rc,Weak}, borrow::{BorrowMut, Borrow}};

use crate::{points::{ColorPoint, Point}, bounding_box::BoundingBox};

//type OctreeLink<'a> = RefCell<Octree<'a>>;
type ParentLink<'a> = RefCell<Weak<Octree<'a>>>;
type ChildLink<'a> = RefCell<Rc<Octree<'a>>>;

pub struct Octree<'a> {
  depth: u8,
  parent: Option<ParentLink<'a>>,
  children: [Option<ChildLink<'a>>; 8],
  bounds: BoundingBox,
  points: HashSet<&'a Point>,
  coord: usize,
  ptr: Option<ParentLink<'a>>,
}

struct Search<'a> {
  canidate: &'a Point,
  source: &'a ColorPoint,
  best_distance_sq: i32,
  bounds: BoundingBox,
}

static QUAD_TUNING: usize = 64;
static TREE_TUNING_DEPTH: u8 = 4;

// trait ChildInstantiator<'a> {
//   fn get_or_create(& mut self, parent: &'a Octree, color: & ColorPoint) -> &mut Box<Octree<'a>>;
//   fn instantiate(& mut self, parent: &'a Octree, color: & ColorPoint) -> &mut Box<Octree<'a>>;
// }

// impl<'a> ChildInstantiator<'a> for Option<Box<Octree<'a>>> {
//   fn get_or_create(& mut self, parent: &'a Octree, color: & ColorPoint) -> &mut Box<Octree<'a>> {
//     match self {
//       None => self.instantiate(parent, color),
//       Some(me) => me
//     }
//   }

//   fn instantiate(& mut self, parent: &Rc<OctreeLink<'a>>, color: & ColorPoint) -> &mut Box<Octree<'a>> {
//     *self = match self {
//       None => {
//         let caddr = parent.addr(color);
//         let radius = i32::from(parent.radius());

//         // Calculate new bounds
//         let bounds = &parent.bounds;
//         let clr = if i32::from(color.r) > bounds.lr + radius { bounds.lr + radius } else { bounds.lr };
//         let cur = if i32::from(color.r) < bounds.ur - radius { bounds.ur - radius } else { bounds.ur };
//         let clg = if i32::from(color.g) > bounds.lg + radius { bounds.lg + radius } else { bounds.lg };
//         let cug = if i32::from(color.g) < bounds.ug - radius { bounds.ug - radius } else { bounds.ug };
//         let clb = if i32::from(color.b) > bounds.lb + radius { bounds.lb + radius } else { bounds.lb };
//         let cub = if i32::from(color.b) < bounds.ub - radius { bounds.ub - radius } else { bounds.ub };


//         Some(Box::new(Octree::new(
//           Some(Box::new(parent)),
//           parent.depth + 1,
//           parent.coord | caddr << (18 - 3 * parent.depth),
//           BoundingBox::new(clr, clg, clb, cur, cug, cub)
//         )))
//       },
//       _ => panic!("Tried to instantiate an existing Octree node")
//     };

//     self.as_mut().unwrap()
//   }
// }

impl<'a> Octree<'a> {
  pub fn new(
    parent: Option<ParentLink<'a>>, depth: u8, coord: usize,
    bounds: BoundingBox
  ) -> Octree<'a> {
    Octree {
      depth,
      // TODO why can't we use the quick array literal here?
      //      Box isn't copyable?
      children: [None, None, None, None, None, None, None, None],
      parent,
      bounds,
      points: HashSet::new(),
      coord,
      ptr: None,
    }
  }

  pub fn set_ptr_to_self(&mut self, ptr_to_self: &ChildLink<'a>) {
    self.ptr = Some(RefCell::new(Rc::downgrade(&ptr_to_self.borrow())));
  }

  pub fn diameter(&self) -> u8 { 256 >> self.depth }
  pub fn radius(&self) -> u8 { 128 >> self.depth }

  pub fn add(&'a mut self, point: &'a Point) {
    // Add to this node
    self.points.insert(point);
    
    if self.depth < TREE_TUNING_DEPTH {
      // Head downwards
      let child = self.get_or_create_child(&point.color);
      child.borrow_mut().add(point);
    }
  }

  pub fn remove(&mut self, point: &'a Point) {
    if !self.points.contains(point) {
      panic!("Removing non-existent point {point}");
    }

    // Remove from children
    if self.depth < TREE_TUNING_DEPTH {
      match self.children[self.addr(&point.color)] {
        Some(child) => child.borrow_mut().remove(point),
        None => panic!("Tried to remove from a non-existant child node")
      }
    }

    // Remove from self
    self.points.remove(point);
  }

  // fn get_or_create_child_inner(&'a self, color: &'a ColorPoint, child: &mut Option<Box<Octree<'a>>>) {
    
    
    
  //   *child = match child {
  //     Some(c) => *child,
  //     None => {
  //       // Calculate new bounds
  //       let bounds = &self.bounds;
  //       let clr = if i32::from(color.r) > bounds.lr + radius { bounds.lr + radius } else { bounds.lr };
  //       let cur = if i32::from(color.r) < bounds.ur - radius { bounds.ur - radius } else { bounds.ur };
  //       let clg = if i32::from(color.g) > bounds.lg + radius { bounds.lg + radius } else { bounds.lg };
  //       let cug = if i32::from(color.g) < bounds.ug - radius { bounds.ug - radius } else { bounds.ug };
  //       let clb = if i32::from(color.b) > bounds.lb + radius { bounds.lb + radius } else { bounds.lb };
  //       let cub = if i32::from(color.b) < bounds.ub - radius { bounds.ub - radius } else { bounds.ub };

  //       Some(Box::from(Octree::new(
  //         Some(Box::new(self)),
  //         self.depth + 1,
  //         self.coord | caddr << (18 - 3 * self.depth),
  //         BoundingBox::new(clr, clg, clb, cur, cug, cub)
  //       )))
  //     }
  //   };

  // }


  fn get_or_create_child(&'a mut self, color: &'a ColorPoint) -> RefCell<Rc<Octree<'a>>> {
    assert!(self.depth < TREE_TUNING_DEPTH, "Should not create a child past the tuning depth");
    
    let caddr = self.addr(color);
    let radius = i32::from(self.radius());

    let child = self.children[caddr].borrow_mut();

    *child = match child {
      Some(c) => *child,
      None => {
        // Calculate new bounds
        let bounds = &self.bounds;
        let clr = if i32::from(color.r) > bounds.lr + radius { bounds.lr + radius } else { bounds.lr };
        let cur = if i32::from(color.r) < bounds.ur - radius { bounds.ur - radius } else { bounds.ur };
        let clg = if i32::from(color.g) > bounds.lg + radius { bounds.lg + radius } else { bounds.lg };
        let cug = if i32::from(color.g) < bounds.ug - radius { bounds.ug - radius } else { bounds.ug };
        let clb = if i32::from(color.b) > bounds.lb + radius { bounds.lb + radius } else { bounds.lb };
        let cub = if i32::from(color.b) < bounds.ub - radius { bounds.ub - radius } else { bounds.ub };

        let child = RefCell::new(Rc::new(Octree::new(
          Some(RefCell::new(Weak::clone(&self.ptr.expect("Self ptr should always be set").borrow()))),
          self.depth + 1,
          self.coord | caddr << (18 - 3 * self.depth),
          BoundingBox::new(clr, clg, clb, cur, cug, cub)
        )));
        child.borrow_mut().set_ptr_to_self(&child);

        Some(child)
      }
    };
    
    //self.get_or_create_child_inner(color, &mut self.children[caddr]);
    let thing = child.expect("Just created a child, it should exist");
    thing.borrow_mut().get_or_create_child(color)
    // let caddr = self.addr(color);
    // let radius = i32::from(self.radius());
    //self.get_or_create_child_inner_v2(&mut self.children, caddr, radius, color);

    //&'a mut self.children[caddr].as_mut().unwrap()
    //self.get_child_mut(color).expect("Just created child node should exist")
    //return self.get_child_mut(color).unwrap();
  }

  fn get_child(&'a self, color: &'a ColorPoint) -> Option<RefCell<Rc<Octree<'a>>>> {
    // match self.children[self.addr(color)] {
    //   Some(child) => Some(Rc::clone(child.borrow())),
    //   None => None, 
    // }

    let child = self.children[self.addr(color)].borrow();

    match child {
      Some(c) => Some(RefCell::new(Rc::clone(&c.borrow()))),
      None => None
    }
  }
  
  // fn get_child_mut(&'a mut self, color: &'a ColorPoint) -> Option<&mut Box<Octree<'a>>> {
  //   self.children[self.addr(color)]?.borrow_mut()
  // }

  fn addr(&'a self, color: &'a ColorPoint) -> usize {
    let mask = self.radius();
    let over = 7 - self.depth;

    let raddr = (color.r & mask) >> over;
    let gaddr = (color.g & mask) >> over;
    let baddr = (color.b & mask) >> over;

    usize::from(raddr << 2 | gaddr << 1 | baddr)
  }

  pub fn find_nearest(&'a self, color: &'a ColorPoint) -> Option<&'a Point> {
    let child = self.get_child(color);

    if self.points.is_empty() {
      panic!("Tried to find nearest but no points at depth {0}", self.depth);
    }

    let have_search_child = match child {
      Some(c) => !c.borrow().points.is_empty(),
      None => false
    };

    if self.points.len() <= QUAD_TUNING || !have_search_child {
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
          source: color,
          best_distance_sq: distance,
          bounds: BoundingBox::from_around(color, search_radius)
        };

        search = self.parent
          .expect("depth > 0 should have a parent")
          .borrow()
          .upgrade()
          .expect("depth > 0 should have a non-deleted parent")
          .nn_search_up(search, self);

        return Some(search.canidate);
      }

      return Some(ret);
    } else {
      return child?.borrow().find_nearest(color);
    }
  }

  fn nearest_in_self(&'a self, color: &'a ColorPoint) -> &'a Point {
    let result = self.points.iter()
      .map(|p| (p, p.color.distance_to(color)))
      .min_by(|a, b| a.1.cmp(&b.1));

    result.expect("Should have had at least one point for nearest_in_self").0
  }

  fn nn_search_up(&'a self, mut search: Search<'a>, from: &'a Octree) -> Search<'a> {
    assert!(search.bounds.intersects(&self.bounds), "Searching up a non-intersecting tree");

    // Search all the children we didn't come from
    for child in &self.children {
      match child {
        None => {},
        Some(c) => {
          let to_search = Rc::clone(&c.borrow());
          if !ptr::eq(to_search.borrow(), from) {
            search = to_search.nn_search_down(search);
          }
        }
      }
    }

    // If the search space is still outside us and we're not root
    if self.depth > 0 && !self.bounds.contains(&search.bounds) {
      search = self.parent
        .expect("depth > 0 should have a parent")
        .borrow()
        .upgrade()
        .expect("depth > 0 should have a parent")
        .nn_search_up(search, self);
    }

    search
  }

  fn nn_search_down(&'a self, mut search: Search<'a>) -> Search<'a> {
    // Skip us if not in search space
    if !search.bounds.intersects(&self.bounds) { return search; }

    // We have no points to search
    if self.points.is_empty() { return search; }

    if self.points.len() <= QUAD_TUNING {
      // We have few enough points, search here
      let our_nearest = self.nearest_in_self(search.source);
      let nearest_dist = search.source.distance_to(&our_nearest.color);

      if nearest_dist < search.best_distance_sq {
        // New candidate!
        search.canidate = our_nearest;
        search.best_distance_sq = nearest_dist;
        search.bounds.set_around(search.source, f64::from(nearest_dist).sqrt().floor() as i32);
      }
    } else if self.depth < TREE_TUNING_DEPTH {
      // Keep going down!
      for child in &self.children {
        match child {
          Some(c) => { search = c.borrow().nn_search_down(search) },
          None => {}
        };
      }
    }

    search
  }
}