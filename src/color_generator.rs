use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering, AtomicU64};
use std::thread;
use std::time::{Instant, Duration};
use std::{path::Path};
use std::fs::File;
use std::io::BufWriter;


use crate::image::Image;
use crate::{points::{ColorPoint, SpacePoint, Point}, octree::Octree, bounding_box::BoundingBox,atomicbitmask::AtomicBitMask};

type SpacePoints = Box<[SpacePoint; 4096*4096]>;

pub struct ColorGenerator {
  colors: Vec<ColorPoint>,
  spaces: SpacePoints,
  writing_spaces: AtomicBitMask,
  written_spaces: AtomicBitMask,
  avail_spaces: AtomicBitMask,
  root: Arc<Octree>,
  image: Image,
  current_color_idx: AtomicUsize,
}



// Public things
impl ColorGenerator {
  pub fn new() -> ColorGenerator {
    ColorGenerator {
      colors: initialize_color_space(),
      spaces: initialize_space_space(),
      current_color_idx: AtomicUsize::new(0),
      image: Image::new(),
      writing_spaces: AtomicBitMask::new(4096*4096),
      written_spaces: AtomicBitMask::new(4096*4096),
      avail_spaces: AtomicBitMask::new(4096*4096),
      root: Octree::new(None, 0, 0, BoundingBox::new(0, 0, 0, 255, 255, 255)),
    }
  }

  pub fn shuffle_colors(&mut self) {
    fastrand::seed(0); // Testing
    fastrand::shuffle(&mut self.colors);
  }

  pub fn add_next_seed_pixel(&self, x: u32, y: u32) {
    let color = &self.colors[self.current_color_idx.fetch_add(1, Ordering::SeqCst)];
    let ofs = space_offset(x, y);

    println!("Seed is {ofs}");

    if self.writing_spaces.test_and_set(ofs) || self.written_spaces.test_and_set(ofs) {
      panic!("Seeded already written point");
    }

    // Write out the pixel
    let space = &self.spaces[ofs];
    self.image.write(space, color);

    // Add our intial neighbors
    let mut add_vec = Vec::with_capacity(4);
    self.add_neighbors(space, color, &mut add_vec);

    // Mark as ready for the workers
    self.avail_spaces.test_and_set(color.offset());

    //self.root.add(Arc::new(Point { color: color.clone(), space: ofs }));
    
  }

  fn add_neighbors(&self, space: &SpacePoint, color: &ColorPoint, add_vec: &mut Vec<SpacePoint>) {
    space.get_neighbors(add_vec);
    for neighbor in add_vec {
      if self.written_spaces.test(neighbor.offset()) || self.writing_spaces.test(neighbor.offset()) {
        continue;
      } else {
        let new_point: Point = Point::new(&neighbor, color);

        // XXX Are we a bad person for this?
        if !self.writing_spaces.test_and_set(neighbor.offset()) {
          self.root.add(new_point);
          assert!(self.writing_spaces.clear(neighbor.offset()), "Somebody cleared our lock");
        } else {
          //println!("We probably just saved your life!");
        }
      }
    }
  }

  /// Attempts to locate the next point to paint to.
  /// Attempts to do this in a thread safe way, reporting potential collisions to color_miss and collision_miss for diagnostics
  fn find_next_point<
    F1 : Fn(Point),
    F2 : Fn(Point)
  >(&self, at: &ColorPoint, color_miss: F1, collision_miss: F2) -> Point {
    loop {
      // Wait for somebody to populate another point
      if self.root.len() == 0 {
        thread::sleep(Duration::from_millis(1));
        continue;
      }

      // Perform the search
      // IMPORTANT: This function may return points that are not fully written yet, or have also been found by somebody else
      // ALSO IMPORTANT: With the new threading impl, somebody might remove a point as we are searching to it, try again for this
      let next = self.root.find_nearest(at);
        //.expect("Tried to add a pixel but there were none to grow on");

      // println!("Found {}", match &next {
      //   None => format!("None"),
      //   Some(p) => format!("{}", p.space)
      // });

      let next = match next {
        None => continue,
        Some(n) => n
      };

      // Don't take if not fully available yet, somebody else might still be working on adding it
      if !self.avail_spaces.test(next.color().offset()) { 
        color_miss(next);
        continue;
      }

      if !self.root.has_point(&next) {
        //println!("Partial color miss?");
        continue;
      }

      // Don't take something somebody else has already claimed, e.g. if we search it up at the same time
      // Note this will atomically claim the point if it is not already
      if !self.writing_spaces.test_and_set(next.space().offset()) { 
        assert!(!self.written_spaces.test(next.space().offset()), "Marked written but found in search");
        break next; 
      }
      

      // Somebody else is already writing to this space, tally it and try again later
      collision_miss(next);
    }
  }

  pub fn grow_pixels_to(self_src: &Arc<RwLock<Self>>, pixel_count: usize) {
    
    if self_src.read().unwrap().current_color_idx.load(Ordering::SeqCst) == 0 {
      panic!("Tried to call grow_pixels_to without any seed pixels");
    }

    println!("Start of the party with {} existing", self_src.read().unwrap().root.len());

    let search_time_src = Arc::new(AtomicU64::default());
    let place_time_src = Arc::new(AtomicU64::default());
    let remove_time_src = Arc::new(AtomicU64::default());
    let add_time_src = Arc::new(AtomicU64::default());
    let color_misses_src = Arc::new(AtomicU64::default());
    let collision_misses_src = Arc::new(AtomicU64::default());

    let wall_start_time = Arc::new(Instant::now());
    
    let mut handles = vec![];
    for thread_id in 0..16 {

      let selfish_src = Arc::clone(self_src);
      
      let search_time = Arc::clone(&search_time_src);
      let place_time = Arc::clone(&place_time_src);
      let remove_time = Arc::clone(&remove_time_src);
      let add_time = Arc::clone(&add_time_src);
      let color_misses = Arc::clone(&color_misses_src);
      let collision_misses = Arc::clone(&collision_misses_src);

      let my_writes = AtomicBitMask::new(4096*4096);
      let my_wall = Arc::clone(&wall_start_time);
      
      let handle = thread::Builder::new().name(format!("Worker {}", thread_id)).spawn(move || {

        let selfish = selfish_src.read().unwrap();
        let mut add_vec = Vec::with_capacity(4);
        
        loop {
          let i = selfish.current_color_idx.fetch_add(1, Ordering::SeqCst);
          if i >= pixel_count {
            // All done
            return;
          }

          // Diagnostics printing
          if i & 262143 == 0 {
            // Progress
            let time_so_far = my_wall.elapsed().as_micros();
            let time_per_px = time_so_far as f64 / i as f64;
            let remaining = time_per_px * (4096 * 4096 - i) as f64;

            println!("Adding pixel {i} ({:.1}%), wf = {}, s={}, p={}, r={}, add={}, mr={} mw={}, ETA={:.2}/{:.2}s as {:.2} kpx/s",
              100.0 * (i as f64) / 4096.0 / 4096.0,
              selfish.root.len(),
              search_time.load(Ordering::Relaxed) / 1000,
              place_time.load(Ordering::Relaxed) / 1000,
              remove_time.load(Ordering::Relaxed) / 1000,
              add_time.load(Ordering::Relaxed) / 1000,
              color_misses.load(Ordering::Relaxed),
              collision_misses.load(Ordering::Relaxed),
              remaining / 1000.0 / 1000.0,
              (remaining + time_so_far as f64) / 1000.0 / 1000.0,
              1000.0 / time_per_px,
              
            );
          }
          
          let at = &selfish.colors[i];

          // Find something we can claim!
          let next = timed(&search_time, || selfish.find_next_point(at,
            |candidate| {
              let last_misses = color_misses.fetch_add(1, Ordering::Relaxed);
              if last_misses & 65535 == 0 { println!("Read misses: {last_misses} {} {} {}", candidate.space(), at, candidate.color()); }
              //println!("Read misses: {last_misses} {} {} {}", candidate.space, at, candidate.color);
            }, |candidate| {
              if my_writes.test(candidate.space().offset()) {
                // So what, we failed to remove one we visited?
                panic!("Search returned point that this thread previously wrote");
              }

              let last_misses = collision_misses.fetch_add(1, Ordering::Relaxed);
              if last_misses & 65535 == 0 { println!("Write misses: {last_misses}/{i}/{}/{} -> {}, of {} in {thread_id}", candidate.space(), at, candidate.color(), 0); }
              // if last_misses > 1024 * 1024 {
              //   panic!("Probably dead");
              // }
            })
          );

          assert!(!my_writes.test_and_set(next.space().offset()), "Local double write");

          // Write this pixel
          let space = timed(&place_time, || {
            let space = &selfish.spaces[next.space().offset()];
            selfish.image.write(space, at);
            space
          });
          
          // Remove this pixel from the tree
          timed(&remove_time, || {
            selfish.root.remove(next);
          });

          
          

          // Sanity
          // assert!(!selfish.root.has(next.space), "Called remove but still present");
          // assert!(!selfish.root.has_point(&next), "Called remove but still present");

          // Add the color as an option on our neighbors
          timed(&add_time, || {
            selfish.add_neighbors(space, at, &mut add_vec)
          });
          
          // Mark write as completed
          assert!(!selfish.written_spaces.test_and_set(next.space().offset()), "double space write");
          // Mark colors as available
          assert!(!selfish.avail_spaces.test_and_set(at.offset()), "double color write");
          
        };
      }).unwrap();

      //handle.join().unwrap();


      handles.push(handle);
    }

    for handle in handles {
      handle.join().unwrap();
    }
  }


  pub fn write_image(&self, path_spec: &String) {
    let path = Path::new(path_spec);
    let file = File::create(path).unwrap();
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, 4096, 4096);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_trns(vec!(0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8));
    encoder.set_source_gamma(png::ScaledFloat::from_scaled(45455)); // 1.0 / 2.2, scaled by 100000
    encoder.set_source_gamma(png::ScaledFloat::new(1.0 / 2.2));     // 1.0 / 2.2, unscaled, but rounded
    let source_chromaticities = png::SourceChromaticities::new(     // Using unscaled instantiation here
        (0.31270, 0.32900),
        (0.64000, 0.33000),
        (0.30000, 0.60000),
        (0.15000, 0.06000)
    );
    encoder.set_source_chromaticities(source_chromaticities);
    let mut writer = encoder.write_header().unwrap();

    let raw = self.image.to_raw();
    writer.write_image_data(raw.as_ref()).unwrap(); // Save
  }
}


fn timed<T, F: FnMut() -> T>(timer: &Arc<AtomicU64>, mut task: F) -> T {
  let start = Instant::now();
  let ret = task();
  timer.fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
  ret
}

/// Sets up our list of colors and color pointers
fn initialize_color_space() -> Vec<ColorPoint> {
  let mut colors = Vec::with_capacity(4096 * 4096);
  
  for r in 0..=255u8 {
    for g in 0..=255u8 {
      for b in 0..=255u8 {
        //let idx = usize::from(r) << 16 | usize::from(g) << 8 | usize::from(b);
        //self.color_space[idx] = &self.colors[idx];

        colors.push(ColorPoint::new(r, g, b));
      }
    }
  }

  colors
}

/// Sets up our list of points
fn initialize_space_space() -> SpacePoints {
  let now = Instant::now();
  
  const ZERO: SpacePoint = SpacePoint(0);
  let mut spaces = box [ZERO; 4096*4096];

  println!("  Alloc spaces in {}", now.elapsed().as_millis());

  for x in 0..4096u32 {
    for y in 0..4096u32 {
      let point = &mut spaces[space_offset(x, y)];

      *point = SpacePoint::new(x, y);
    }
  }

  println!("  Init spaces in {}", now.elapsed().as_millis());


  spaces//.map(|s| Arc::new(s))
}

// Helpers

/// Converts an x/y coordinate to an index
/// TODO get in sync with the SpacePoint one...
fn space_offset(x: u32, y: u32) -> usize {
  usize::try_from(y << 12 | x).expect("Should not index a point beyond 2^24")
}