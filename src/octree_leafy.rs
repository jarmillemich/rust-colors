use std::{sync::{Arc, atomic::{AtomicUsize, Ordering}}};

use integer_sqrt::IntegerSquareRoot;
use parking_lot::RwLock;

use crate::{points::{SpacePoint, Point, ColorPoint}, bounding_box::BoundingBox, nn_search_3d::NnSearch3d};

type LeafBucket = Vec<Point>;
type LeafBucketWrapper = Arc<RwLock<LeafBucket>>;

/*
    We've cleaned up most of the atomics, but we are still using them for total_points, how best to avoid this?
        These counts are used to exclude nodes from searches and save a lot of time
    We also have locks on the leaf nodes, but this should not be contentious?
*/

pub enum OctreeLeafy {
    Node {
        children: [Box<OctreeLeafy>; 8],
        bounds: BoundingBox,
        depth: usize,
        total_points: AtomicUsize,
    },
    Leaf {
        points: LeafBucketWrapper,
        bounds: BoundingBox,
        total_points: AtomicUsize,
    },
}

struct NearestSearch {
    pub nearest: Point,
    pub nearest_dist: i32,
    pub bounds: BoundingBox,
}

impl OctreeLeafy {
    pub fn init_tree(depth: usize) -> OctreeLeafy {
        assert!(depth > 0, "Tried to init tree with depth 0 (must be > 0");
        assert!(depth < 8, "Tried to init tree with depth {depth} (must be < 8)");
        //assert!(depth < 4, "Tried to init tree with depth {depth} (must be < 4 as we're aggressive with pre-allocations atm)");

        Self::init_node(0, depth, BoundingBox::new(
            0, 0, 0, 255, 255, 255,
        ))
    }

    fn init_node(depth: usize, remaining_depth: usize, bounding_box: BoundingBox) -> OctreeLeafy {
        if remaining_depth == 0 {
            OctreeLeafy::Leaf {
                points: Arc::new(RwLock::new(Vec::new())),
                bounds: bounding_box,
                total_points: AtomicUsize::new(0),
            }
        } else {
            let sub_radius = 128 >> depth;

            OctreeLeafy::Node {
                // Subdivision packing is RGB ---, --+, -+-, -++, +--, +-+, ++-, +++
                children: [
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(0, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(1, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(2, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(3, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(4, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(5, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(6, sub_radius))),
                    Box::new(Self::init_node(depth + 1, remaining_depth - 1, bounding_box.sub_for_idx(7, sub_radius))),
                ],
                bounds: bounding_box,
                depth,
                total_points: 0.into(),
            }
        }
    }

    pub fn radius(depth: usize) -> i32 { 
        128 >> depth
    }

    fn addr(depth: usize, color: &ColorPoint) -> usize {
        // Subdivision packing is RGB ---, --+, -+-, -++, +--, +-+, ++-, +++
        let mask = Self::radius(depth);
        let over = 7 - depth;
    
        let addr_red = (color.r as i32 & mask) >> over;
        let addr_green = (color.g as i32 & mask) >> over;
        let addr_blue = (color.b as i32 & mask) >> over;
    
        (addr_red << 2 | addr_green << 1 | addr_blue) as usize
    }

    fn child_for(&self, color: &ColorPoint) -> Option<&OctreeLeafy> {
        match self {
            OctreeLeafy::Node { children, depth, .. } => {
                let addr = Self::addr(*depth, color);
                Some(&children[addr])
            }
            OctreeLeafy::Leaf { .. } => {
                None
            }
        }
    }

    /// Grabs the first point we can find below us
    fn first_point(&self) -> Option<Point> {
        if self.is_empty() {
            return None;
        }

        match self {
            OctreeLeafy::Node { children, .. } => {
                children.iter()
                    .find_map(|child| child.first_point())
            }
            OctreeLeafy::Leaf { points, .. } => {
                points.read()
                    .first()
                    .cloned()
            }
        }
    }

    fn intersects(&self, bounds: &BoundingBox) -> bool {
        match self {
            OctreeLeafy::Node { bounds: node_bounds, .. } => {
                node_bounds.intersects(bounds)
            }
            OctreeLeafy::Leaf { bounds: leaf_bounds, .. } => {
                leaf_bounds.intersects(bounds)
            }
        }
    }

    #[inline(never)]
    fn find_nearest_inner(&self, pt: &ColorPoint, search: &mut NearestSearch) {
        match self {
            OctreeLeafy::Node { children, .. } => Self::find_nearest_inner_node(pt, children, search),
            OctreeLeafy::Leaf { points, .. } => Self::find_nearest_inner_leaf(pt, points, search),
        }
    }

    // Breaking these two out for performance profiling reasons
    // TODO something with cfg_attr

    #[inline(never)]
    fn find_nearest_inner_node(pt: &ColorPoint, children: &[Box<OctreeLeafy>; 8], search: &mut NearestSearch) {
        for child in children {
            if child.is_empty() || !child.intersects(&search.bounds) {
                // Don't bother
                continue;
            }

            child.find_nearest_inner(pt, search);
        }
    }

    #[inline(never)]
    fn find_nearest_inner_leaf(pt: &ColorPoint, points: &LeafBucketWrapper, search: &mut NearestSearch) {
        // Check all of our points and update the search if we find a better one
        // If we have some equal points, choose a random one?
        // let mut candidates = Vec::with_capacity(4);

        for point in points.read().iter() {

            if !search.bounds.contains_color(point.color()) {
                // Quickly exclude if outside the search area
                continue;
            }

            let dist = point.color().distance_to(pt);

            if dist == 0 {
                // This is it
                search.nearest.clone_from(point);
                search.nearest_dist = 0;
                return;
            }

            if dist == search.nearest_dist {
                // candidates.push(point.clone());
            }

            if dist < search.nearest_dist {
                search.nearest.clone_from(point);
                search.nearest_dist = dist;
                let dist_actual = dist.integer_sqrt();
                //search.bounds.set_around(pt, f64::from(search.nearest_dist).sqrt().floor() as i32);
                search.bounds.set_around(pt, dist_actual);

                // candidates.clear();
                // candidates.push(point.clone());
            }
        }

        // if candidates.len() > 1 {
        //     // Choose a random one
        //     search.nearest = candidates.choose(&mut rand::thread_rng()).unwrap().clone();
        // }
    }

    // Testing how we might do the recursion part on child threads and just the final write on the main thread
    pub fn precalc_path(&self, point: Point) -> LeafBucketWrapper {
        let mut at = self;
        let color = &point.color();

        loop {
            match at {
                OctreeLeafy::Node { .. } => {
                    // Descend
                    at = at.child_for(color).unwrap();
                }
                OctreeLeafy::Leaf { points, .. } => {
                    // No more to do
                    return points.clone();
                }
            }
        }

    }
}

impl NnSearch3d for OctreeLeafy {
    fn has(&self, pt: &SpacePoint) -> bool {
        match self {
            OctreeLeafy::Node { children, .. } => {
                children.iter().any(|child| child.has(pt))
            }
            OctreeLeafy::Leaf { points, .. } => {
                points.read().iter().any(|p| p.space() == pt)
            }
        }
    }

    fn has_point(&self, pt: &Point) -> bool {
        match self {
            OctreeLeafy::Node { .. } => {
                // Go to child by color
                self.child_for(&pt.color()).unwrap().has_point(pt)
            }
            OctreeLeafy::Leaf { points, .. } => {
                points.read()
                    .iter()
                    .any(|p| p == pt)
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            OctreeLeafy::Node { total_points, .. } => total_points.load(Ordering::Relaxed), // XXX
            OctreeLeafy::Leaf { total_points, .. } => total_points.load(Ordering::Relaxed), // XXX
        }
    }

    fn add(&self, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
        match self {
            OctreeLeafy::Node { ref total_points, .. } => {
                // Materialize that we added a point
                total_points.fetch_add(1, Ordering::Relaxed); // XXX
                // Add to child by color
                let child = self.child_for(&point.color()).unwrap();
                child.add(point, spare_vectors);
            }
            OctreeLeafy::Leaf { points, total_points, .. } => {
                let mut lock = points.write();
                // TODO check we don't already have it? we shouldn't
                lock.push(point);
                total_points.fetch_add(1, Ordering::Relaxed); // XXX
            }
        }
    }

    fn remove(&self, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
        match self {
            OctreeLeafy::Node { ref total_points, .. } => {
                // Materialize that we removed a point
                total_points.fetch_sub(1, Ordering::Relaxed); // XXX
                // Remove from child by color
                let child = self.child_for(&point.color()).unwrap();
                child.remove(point, spare_vectors);
            }
            OctreeLeafy::Leaf { points, total_points, .. } => {
                
                let before = points.read().len();
                points.write().retain(|p| p != &point);
                total_points.fetch_sub(before - points.read().len(), Ordering::Relaxed); // XXX
                
            }
        }
    }

    fn find_nearest(&self, color: &ColorPoint) -> Option<Point> {
        // Start with the smallest node around our target that contains any point
        let mut at = self;
        while let Some(next) = at.child_for(color) {
            // Nothing at or below us, so we're done
            if next.is_empty() {
                break;
            }

            at = next;
        }

        // Grab the (hopefully nearby) starting point
        let Some(nearest) = at.first_point() else { return None };
        let nearest_dist = nearest.color().distance_to(color);

        if nearest_dist == 0 {
            // We simply can't do better than that!
            return Some(nearest);
        }

        // Set the search bounds to be around the starting point
        // The radius to beat is of course the distance to this starting point
        let bounds = BoundingBox::from_around(color, f64::from(nearest_dist).sqrt().floor() as i32);
        
        let mut search = NearestSearch {
            nearest,
            nearest_dist,
            bounds,
        };

        self.find_nearest_inner(color, &mut search);
        
        Some(search.nearest)
    }

    #[inline(never)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[test]
fn test_octree_leafy_add_remove() {
    let tree = OctreeLeafy::init_tree(3);
    let mut spare_vectors = Vec::new();
    assert!(tree.is_empty());

    let point = Point::new(SpacePoint::new(0, 0), ColorPoint::new(0, 0, 0));
    tree.add(point.clone(), &mut spare_vectors);
    assert!(!tree.is_empty());

    assert!(tree.has_point(&point));
    assert!(tree.has(point.space()));

    tree.remove(point.clone(), &mut spare_vectors);
    assert!(tree.is_empty());

    assert!(!tree.has_point(&point));
    assert!(!tree.has(point.space()));
}

#[test]
fn test_octree_init_bounds() {
    let tree = OctreeLeafy::init_tree(2);
    
    let OctreeLeafy::Node { bounds, .. } = &tree else { panic!("Root should be node") };

    assert_eq!(bounds, &BoundingBox::new(0, 0, 0, 255, 255, 255));

    let colors_to_check = [
        ColorPoint::new(0, 0, 0),
        ColorPoint::new(255, 255, 255),
        ColorPoint::new(0, 0, 255),
        ColorPoint::new(0, 255, 0),
        ColorPoint::new(0, 255, 255),
        ColorPoint::new(255, 0, 0),
        ColorPoint::new(255, 0, 255),
        ColorPoint::new(255, 255, 0),
    ];
    
    for color in colors_to_check.iter() {
        let child = tree.child_for(color);
        let Some(OctreeLeafy::Node { bounds, .. }) = child
            else { panic!("Child should be node") };
        
        assert!(bounds.contains_color(&color), "Child node for color {color:?} should contain it, but bounds are {bounds:?}");
    }
}

#[test]
fn test_octree_find_nearest_single() {
    let tree = OctreeLeafy::init_tree(2);
    let mut spare_vectors = Vec::new();
    
    let point = Point::new(SpacePoint::new(0, 0), ColorPoint::new(0, 0, 0));
    tree.add(point.clone(), &mut spare_vectors);

    let colors_to_check = [
        ColorPoint::new(0, 0, 0),
        ColorPoint::new(255, 255, 255),
        ColorPoint::new(0, 0, 255),
        ColorPoint::new(0, 255, 0),
        ColorPoint::new(0, 255, 255),
        ColorPoint::new(255, 0, 0),
        ColorPoint::new(255, 0, 255),
        ColorPoint::new(255, 255, 0),
    ];

    // With only one sample point, all should just find it
    for color in colors_to_check.iter() {
        let nearest = tree.find_nearest(color);
        assert_eq!(nearest, Some(point.clone()));
    }
}

#[test]
fn test_octree_find_nearest_multi() {
    // Have a tree with several points in it and try NN search
    let tree = OctreeLeafy::init_tree(4);
    let mut spare_vectors = Vec::new();

    let placed_points = [
        // let c = () => Math.floor(Math.random() * 256)
        // for (let i = 0; i < 16; i++) console.log(`ColorPoint::new(${c()}, ${c()}, ${c()}),`)
        ColorPoint::new(15, 118, 246),
        ColorPoint::new(39, 85, 206),
        ColorPoint::new(108, 135, 90),
        ColorPoint::new(249, 228, 159),
        ColorPoint::new(83, 27, 105),
        ColorPoint::new(20, 198, 200),
        ColorPoint::new(99, 184, 189),
        ColorPoint::new(87, 221, 39),
        ColorPoint::new(148, 27, 114),
        ColorPoint::new(94, 189, 2),
        ColorPoint::new(88, 186, 237),
        ColorPoint::new(162, 144, 96),
        ColorPoint::new(195, 95, 154),
        ColorPoint::new(246, 14, 205),
        ColorPoint::new(238, 40, 80),
        ColorPoint::new(183, 146, 75),
    ];

    for pt in placed_points {
        // Just use 0, 0 for space
        let point = Point::new(SpacePoint::new(0, 0), pt);
        tree.add(point, &mut spare_vectors);
    }

    let search_points = [
        ColorPoint::new(50, 6, 84),
        ColorPoint::new(62, 93, 91),
        ColorPoint::new(224, 185, 93),
        ColorPoint::new(209, 17, 203),
        ColorPoint::new(134, 202, 34),
        ColorPoint::new(43, 153, 89),
        ColorPoint::new(110, 142, 160),
        ColorPoint::new(116, 107, 233),
        ColorPoint::new(38, 196, 2),
        ColorPoint::new(240, 20, 107),
        ColorPoint::new(233, 56, 187),
        ColorPoint::new(248, 8, 36),
        ColorPoint::new(51, 202, 123),
        ColorPoint::new(20, 65, 92),
        ColorPoint::new(247, 3, 245),
        ColorPoint::new(192, 158, 162),
    ];

    for search_color in search_points {
        // Brute force to find the actual nearest
        let nearest_control = placed_points.iter().min_by(|a, b| {
            let a_dist = a.distance_to(&search_color);
            let b_dist = b.distance_to(&search_color);
            a_dist.partial_cmp(&b_dist).unwrap()
        }).unwrap();

        let nearest = tree.find_nearest(&search_color);

        assert!(nearest.is_some(), "Nearest should be found for color {:?}", search_color);

        let nearest_color = *nearest.unwrap().color();

        assert_eq!(
            nearest_color,
            *nearest_control,
            "Search color {:?} should find nearest {:?} at {}, but found {:?} at {}",
            search_color,
            nearest_control, nearest_control.distance_to(&search_color),
            nearest_color, nearest_color.distance_to(&search_color)
        );
    }
}
