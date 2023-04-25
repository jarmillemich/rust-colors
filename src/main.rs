use std::{time::Instant, mem, sync::{atomic::AtomicU8, Arc, RwLock}};

use rust_colors::{color_generator::ColorGenerator};


fn main() {
    let start = Instant::now();

    println!("{}", mem::size_of::<AtomicU8>());
    
    println!("Starting");

    let mut generator = Box::new(ColorGenerator::new());

    let elapsed = start.elapsed();
    println!("Init Generator at {}", elapsed.as_millis());

    generator.shuffle_colors();
    let elapsed = start.elapsed();
    println!("Shuffle at {}", elapsed.as_millis());

    // whatever.read().unwrap().add_next_seed_pixel(1024, 1024);
    // whatever.read().unwrap().add_next_seed_pixel(3072, 1024);
    // whatever.read().unwrap().add_next_seed_pixel(1024, 3072);
    // whatever.read().unwrap().add_next_seed_pixel(3072, 3072);
    generator.add_next_seed_pixel(2048, 2048, &mut Vec::with_capacity(4));
    let elapsed = start.elapsed();
    println!("Add seed at {}", elapsed.as_millis());

    generator.grow_pixels_to(4096*4096);
    // generator.grow_pixels_to(16*4096);
    let elapsed = start.elapsed();
    println!("Grown at {}", elapsed.as_millis());

    generator.write_image(&String::from("./test.png"));
    let elapsed = start.elapsed();
    println!("Wrote at {}", elapsed.as_millis());

    println!("All done");
}

#[test]
fn test_octree_search_performance() {
    use rand::Rng;
    use rust_colors::{octree_leafy::OctreeLeafy, nn_search_3d::NnSearch3d, points::{ColorPoint, Point, SpacePoint}};

    // Have a tree with several thousand points in it and do many searches to check performance
    let mut tree = OctreeLeafy::init_tree(3);
    let mut spare_vectors = Vec::new();

    let mut rng = rand::thread_rng();

    for i in 0..2000 {
        let point = Point::new(
            &SpacePoint::new(i, i), 
            &ColorPoint::new(rng.gen_range(0..=255), rng.gen_range(0..=255), rng.gen_range(0..=255))
        );
        tree.add_sync(point, &mut spare_vectors);
    }

    // Search random points
    // Optimization hack to actually do the work
    let mut junk = 0;
    for _ in 0..10_000 {
        let search_color = ColorPoint::new(rng.gen_range(0..=255), rng.gen_range(0..=255), rng.gen_range(0..=255));
        let nearest = tree.find_nearest(&search_color);
        assert!(nearest.is_some(), "Nearest should be found for color {:?}", search_color);
        junk += nearest.unwrap().color().r as u64;
    }
    println!("Junk: {}", junk);
}

#[test]
fn test_octree_add_remove_performance() {
    use rand::Rng;
    use rust_colors::{octree_leafy::OctreeLeafy, nn_search_3d::NnSearch3d, points::{ColorPoint, Point, SpacePoint}};

    // Have a tree with several thousand points in it and do many searches to check performance
    let mut tree = OctreeLeafy::init_tree(3);
    let mut spare_vectors = Vec::new();

    let mut rng = rand::thread_rng();
    let mut points = Vec::new();

    // Add some to start
    for i in 0..2000 {
        let point = Point::new(
            &SpacePoint::new(i, i), 
            &ColorPoint::new(rng.gen_range(0..=255), rng.gen_range(0..=255), rng.gen_range(0..=255))
        );
        points.push(point.clone());
        tree.add_sync(point, &mut spare_vectors);
    }

    // Our vector pool
    let mut spare_vectors = vec![Vec::with_capacity(8); 1024];

    // Add/remove for a while
    let mut junk = 0;
    for _ in 0..4096*4096 {
        // Boring pop random
        let index = rng.gen_range(0..points.len());
        let num_points = points.len();
        points.swap(index, num_points - 1);
        let point = points.pop().unwrap();

        //tree.remove_sync(point);
        tree.remove_calculated(tree.precalc_path(point), point, &mut spare_vectors);

        // Add another back
        let point = Point::new(
            &SpacePoint::new(rng.gen_range(0..=4095), rng.gen_range(0..=4095)), 
            &ColorPoint::new(rng.gen_range(0..=255), rng.gen_range(0..=255), rng.gen_range(0..=255))
        );
        points.push(point.clone());
        //tree.add_sync(point);
        tree.add_calculated(tree.precalc_path(point), point, &mut spare_vectors);

        junk += tree.len() as u64;
    }
    println!("Junk: {}", junk);
}