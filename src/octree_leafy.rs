use std::sync::Arc;

use parking_lot::RwLock;

use crate::{points::{SpacePoint, Point, ColorPoint}, bounding_box::BoundingBox, nn_search_3d::NnSearch3d};

type HashMap<K, V> = fnv::FnvHashMap<K, V>;
type LeafBucket = HashMap<SpacePoint, Vec<Point>>;
type LeafBucketWrapper = Arc<RwLock<LeafBucket>>;

pub enum OctreeLeafy {
    Node {
        children: [Box<OctreeLeafy>; 8],
        bounds: BoundingBox,
        depth: usize,
        total_points: usize,
    },
    Leaf {
        points: LeafBucketWrapper,
        spare_vectors: Vec<Vec<Point>>,
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
        //assert!(depth < 8, "Tried to init tree with depth {depth} (must be < 8)");
        assert!(depth < 4, "Tried to init tree with depth {depth} (must be < 4 as we're aggressive with pre-allocations atm)");

        Self::init_node(0, depth, BoundingBox::new(
            0, 0, 0, 255, 255, 255,
        ))
    }

    fn init_node(depth: usize, remaining_depth: usize, bounding_box: BoundingBox) -> OctreeLeafy {
        if remaining_depth == 0 {
            OctreeLeafy::Leaf {
                points: Arc::new(RwLock::new(HashMap::default())),
                // Bulk allocate some vecs to use later
                spare_vectors: vec![Vec::with_capacity(8); 1024],
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
                total_points: 0,
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

    fn child_for(&mut self, color: &ColorPoint) -> Option<&mut OctreeLeafy> {
        match self {
            OctreeLeafy::Node { children, depth, .. } => {
                let addr = Self::addr(*depth, color);
                Some(&mut children[addr])
            }
            OctreeLeafy::Leaf { .. } => {
                None
            }
        }
    }

    fn child_for_shared(&self, color: &ColorPoint) -> Option<&OctreeLeafy> {
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
                points.read().values()
                    .next()
                    .and_then(|points| points.first())
                    .cloned()
            }
        }
    }

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
            if child.is_empty() {
                // Don't bother
                continue;
            }

            // TODO this doesn't copy does it?
            match &**child {
                OctreeLeafy::Node { bounds, .. } => {
                    if bounds.intersects(&search.bounds) {
                        child.find_nearest_inner(pt, search);
                    }
                },
                OctreeLeafy::Leaf { .. } => {
                    child.find_nearest_inner(pt, search);
                }
            }
        }
    }

    #[inline(never)]
    fn find_nearest_inner_leaf(pt: &ColorPoint, points: &LeafBucketWrapper, search: &mut NearestSearch) {
        // Check all of our points and update the search if we find a better one
        for point in points.read().values().flatten() {
            if !search.bounds.contains_color(&point.color()) {
                // Quickly exclude if outside the search area
                continue;
            }

            let dist = point.color().distance_to(pt);

            if dist == 0 {
                // This is it
                search.nearest = point.clone();
                search.nearest_dist = 0;
                return;
            }

            if dist < search.nearest_dist {
                search.nearest = point.clone();
                search.nearest_dist = dist;
                search.bounds.set_around(pt, f64::from(search.nearest_dist).sqrt().floor() as i32);
            }
        }
    }

    // Testing how we might do the recursion part on child threads and just the final write on the main thread
    pub fn precalc_path(&self, point: Point) -> LeafBucketWrapper {
        let mut at = self;
        let color = &point.color();

        loop {
            match at {
                OctreeLeafy::Node { .. } => {
                    // Descend
                    at = at.child_for_shared(color).unwrap();
                }
                OctreeLeafy::Leaf { points, .. } => {
                    // No more to do
                    return points.clone();
                }
            }
        }

    }

    pub fn add_calculated(&mut self, bucket: LeafBucketWrapper, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
        // Add to the hash
        let space = point.space();
        let mut lock = bucket.write();
        let entry = lock.entry(space);
        let points = entry.or_insert_with_key(|_| spare_vectors.pop().unwrap_or_default());
        points.push(point);
    }

    pub fn remove_calculated(&mut self, bucket: LeafBucketWrapper, point: Point, spare_vectors: &mut Vec<Vec<Point>>) {
        // Remove from the hash
        let space = point.space();
        let mut lock = bucket.write();
        let entry = lock.entry(space);

        // Remove this point
        entry.and_modify(|points| {
            points.retain(|p| p != &point);
        });

        // If we ended up empty, recycle!
        if lock.get(&space).unwrap().is_empty() {
            spare_vectors.push(lock.remove(&space).unwrap());
        }
    }
}

impl NnSearch3d for OctreeLeafy {
    fn has(&self, pt: SpacePoint) -> bool {
        match self {
            OctreeLeafy::Node { children, .. } => {
                children.iter().any(|child| child.has(pt))
            }
            OctreeLeafy::Leaf { points, .. } => {
                points.read().contains_key(&pt)
            }
        }
    }

    fn has_point(&self, pt: &Point) -> bool {
        match self {
            OctreeLeafy::Node { .. } => {
                // Go to child by color
                self.child_for_shared(&pt.color()).unwrap().has_point(pt)
            }
            OctreeLeafy::Leaf { points, .. } => {
                points.read().get(&pt.space())
                    .map(|points| points.contains(pt))
                    .unwrap_or(false)
            }
        }
    }

    fn add(&self, _point: Point) {
        unimplemented!("Shared add is not supported for leafy octree")
    }

    fn add_sync(&mut self, point: Point) {
        match self {
            OctreeLeafy::Node { ref mut total_points, .. } => {
                // Materialize that we added a point
                *total_points += 1;
                // Add to child by color
                let child = self.child_for(&point.color()).unwrap();
                child.add_sync(point);
            }
            OctreeLeafy::Leaf { points, spare_vectors } => {
                let space = point.space();
                let mut lock = points.write();
                let entry = lock.entry(space);
                let points = entry.or_insert_with_key(|_| spare_vectors.pop().unwrap_or_default());
                points.push(point);
            }
        }
    }

    fn remove(&self, _point: Point) {
        unimplemented!("Shared remove is not supported for leafy octree")
    }

    fn remove_sync(&mut self, point: Point) {
        match self {
            OctreeLeafy::Node { ref mut total_points, .. } => {
                // Materialize that we removed a point
                *total_points -= 1;
                // Remove from child by color
                let child = self.child_for(&point.color()).unwrap();
                child.remove_sync(point);
            }
            OctreeLeafy::Leaf { points, spare_vectors } => {
                let space = point.space();
                if let Some(points) = points.write().get_mut(&space) {
                    points.retain(|p| p != &point);
                }

                // Remove from the hash if we're empty?
                if points.read()[&space].is_empty() {
                    let pt = points.write().remove(&space);
                    pt.map(|pt| spare_vectors.push(pt));
                }
            }
        }
    }

    fn find_nearest(&self, color: &ColorPoint) -> Option<Point> {
        // Start with the smallest node around our target that contains any point
        let mut at = self;
        while let Some(next) = at.child_for_shared(color) {
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

    fn len(&self) -> usize {
        // TODO probably materialize this? sounds expensive...
        match self {
            OctreeLeafy::Node { total_points, .. } => {
                *total_points
            }
            OctreeLeafy::Leaf { points, .. } => {
                points.read().values().map(|points| points.len()).sum()
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            OctreeLeafy::Node { total_points, .. } => {
                *total_points == 0
            }
            OctreeLeafy::Leaf { points, .. } => {
                // TODO we might want to keep around the Vecs in the hash, do we have to scan them all?
                //      probably we'll want to materialize the counts...
                points.read().is_empty()
            }
        }
    }
}

#[test]
fn test_octree_leafy_add_remove() {
    let mut tree = OctreeLeafy::init_tree(3);
    assert_eq!(tree.len(), 0);
    assert!(tree.is_empty());

    let point = Point::new(&SpacePoint::new(0, 0), &ColorPoint::new(0, 0, 0));
    tree.add_sync(point.clone());
    assert_eq!(tree.len(), 1);
    assert!(!tree.is_empty());

    assert!(tree.has_point(&point));
    assert!(tree.has(point.space()));

    tree.remove_sync(point.clone());
    assert_eq!(tree.len(), 0);
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
        let child = tree.child_for_shared(color);
        let Some(OctreeLeafy::Node { bounds, .. }) = child
            else { panic!("Child should be node") };
        
        assert!(bounds.contains_color(&color), "Child node for color {color:?} should contain it, but bounds are {bounds:?}");
    }
}

#[test]
fn test_octree_find_nearest_single() {
    let mut tree = OctreeLeafy::init_tree(2);
    
    let point = Point::new(&SpacePoint::new(0, 0), &ColorPoint::new(0, 0, 0));
    tree.add_sync(point.clone());

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
    let mut tree = OctreeLeafy::init_tree(7);

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
        let point = Point::new(&SpacePoint::new(0, 0), &pt);
        tree.add_sync(point);
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

        let nearest_color = nearest.unwrap().color();

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

// #[test]
// fn test_octree_search_performance() {
//     use rand::Rng;

//     // Have a tree with several thousand points in it and do many searches to check performance
//     let mut tree = OctreeLeafy::init_tree(7);

//     let mut rng = rand::thread_rng();

//     for i in 0..2000 {
//         let point = Point::new(
//             &SpacePoint::new(i, i), 
//             &ColorPoint::new(rng.gen_range(0..=255), rng.gen_range(0..=255), rng.gen_range(0..=255))
//         );
//         tree.add_sync(point);
//     }

//     // Search random points
//     // Optimization hack to actually do the work
//     let mut junk = 0;
//     for _ in 0..10_000 {
//         let search_color = ColorPoint::new(rng.gen_range(0..=255), rng.gen_range(0..=255), rng.gen_range(0..=255));
//         let nearest = tree.find_nearest(&search_color);
//         assert!(nearest.is_some(), "Nearest should be found for color {:?}", search_color);
//         junk += nearest.unwrap().color().r as u64;
//     }
//     println!("Junk: {}", junk);
//     panic!()
// }