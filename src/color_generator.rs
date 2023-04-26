use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Instant};
use std::{path::Path};
use std::fs::File;
use std::io::BufWriter;
use log::{trace};

use bitvec::prelude::*;
use spmc::Receiver;


use crate::image::Image;
use crate::nn_search_3d::NnSearch3d;
use crate::octree_leafy::OctreeLeafy;
use crate::{points::{ColorPoint, SpacePoint, Point}};

type SpacePoints = Box<Vec<SpacePoint>>;

pub struct ColorGenerator {
  colors: Vec<ColorPoint>,
  spaces: SpacePoints,
  writing_spaces: Box<[usize]>,
  written_spaces: Box<[usize]>,
  root: Arc<OctreeLeafy>,
  image: Image,
  current_color_idx: usize,
  space_mapping: HashMap<SpacePoint, Vec<ColorPoint>>,
}

fn make_boxed_bit_array() -> Box<[usize]> {
  // Silliness to start it on the heap
  vec![0usize; 4096 * 4096 / std::mem::size_of::<usize>()].into_boxed_slice()
}

// Public things
impl ColorGenerator {
  pub fn new() -> ColorGenerator {
    ColorGenerator {
      colors: initialize_color_space(),
      spaces: initialize_space_space(),
      current_color_idx: 0,
      image: Image::new(),
      writing_spaces: make_boxed_bit_array(),
      written_spaces: make_boxed_bit_array(),
      //root: Octree::new(None, 0, 0, BoundingBox::new(0, 0, 0, 255, 255, 255)),
      root: OctreeLeafy::init_tree(4).into(),
      space_mapping: HashMap::new(),
    }
  }

  pub fn shuffle_colors(&mut self) {
    fastrand::seed(0); // Testing
    fastrand::shuffle(&mut self.colors);
  }

  pub fn add_next_seed_pixel(&mut self, x: u32, y: u32, point_pool: &mut Vec<Vec<Point>>) {
    let color = self.colors[self.current_color_idx].clone();
    self.current_color_idx += 1;
    let ofs = space_offset(x, y);

    println!("Seed is {ofs}");

    if self.writing_spaces.view_bits::<Msb0>()[ofs] || self.written_spaces.view_bits::<Msb0>()[ofs] {
      panic!("Seeded already written point");
    }

    self.writing_spaces.view_bits_mut::<Msb0>().set(ofs, true);
    self.written_spaces.view_bits_mut::<Msb0>().set(ofs, true);

    // Write out the pixel
    let space = self.spaces[ofs].clone();
    self.image.write(&space, &color);

    // Add our initial neighbors
    let mut add_vec = Vec::with_capacity(4);
    self.add_neighbors(&space, &color, &mut add_vec, point_pool);

  }

  fn add_neighbors(&mut self, space: &SpacePoint, color: &ColorPoint, add_vec: &mut Vec<SpacePoint>, point_pool: &mut Vec<Vec<Point>>) {
    space.get_neighbors(add_vec);
    for neighbor in add_vec {
      if self.written_spaces.view_bits::<Msb0>()[neighbor.offset()] || self.writing_spaces.view_bits::<Msb0>()[neighbor.offset()] {
        // Already occupied
        continue;
      } else {
        let new_point = Point::new(*neighbor, *color);

        self.space_mapping.entry(neighbor.clone()).or_insert(Vec::new()).push(color.clone());
        println!("Adding {new_point} (seed)");
        self.root.add(new_point, point_pool)
      }
    }
  }


  pub fn grow_pixels_to(&mut self, pixel_count: usize) {


    // FUTURE so the new plan is to have a bunch of search threads and mutation threads
    // search threads will search for the next point to mutate, and then send it to the main thread
    // The main thread gatekeeps items from the search threads, so points are only dispatched once
    // The main thread dispatches ok points to the mutation threads on a spmc so they can grab some when available
    // Worker threads also report their performance metrics somehow???
    // Just gotta keep the book-keeping performant
    
    if self.current_color_idx == 0 {
      panic!("Tried to call grow_pixels_to without any seed pixels");
    }

    println!("Start of the party with {} existing", self.root.len());

    // Diagnostic timers, these are all in microseconds
    let mut search_time_src: usize = 0;
    let mut place_time_src: usize = 0;
    let mut remove_time_src: usize = 0;
    let mut add_time_src: usize = 0;
    let color_misses_src: usize = 0;
    let mut collision_misses_src: usize = 0;

    let wall_start_time = Instant::now();

    // Search threads
    let num_search_threads = 4;
    let mut search_handles = vec![];
    // For sending colors to the search threads
    let (mut tx_search_send, rx_search_send) = spmc::channel();
    // For receiving search results from the search threads
    let (tx_search_receive, rx_search_receive) = mpsc::channel();

    // Mutation threads
    let num_mutation_threads = 1;
    let mut mutation_handles = vec![];
    // For sending results to the mutation threads
    let (mut tx_mutation_send, rx_mutation_send) = spmc::channel();
    // Just for stats atm
    let (tx_mutation_receive, rx_mutation_receive) = mpsc::channel();

    // Spawn the search threads
    for thread_id in 0..num_search_threads {
      let rx_search_send = rx_search_send.clone();
      let tx_search_receive = tx_search_receive.clone();
      let root = self.root.clone();

      search_handles.push(thread::Builder::new().name(format!("Searcher {}", thread_id)).spawn(move || {
        let mut backfill = Vec::with_capacity(32);
        
        loop {
          // Get the next color to search for
          let color = if backfill.is_empty() {
            // If no backfill block on our receiver
            let Some(next) = rx_search_send.recv().ok() else {
              // If the receiver is empty, we're out of work
              println!("Search thread {} exiting, out of work", thread_id);
              break;
            };

            next
          } else {
            // If we do have some backfill, try the receiver first but then use the backfill
            let Some(next) = rx_search_send.try_recv().ok().or_else(|| backfill.pop()) else {
              continue;
            };

            next
          };

          // Search for the next point
          // NB find_nearest will return None if a point was removed during our search
          let start = Instant::now();
          let next = root.find_nearest(&color);
          let end = Instant::now();
          let search_time = end.duration_since(start).as_micros() as usize;

          match next {
            Some(next) => {
              // We found a point, send it to the main thread
              tx_search_receive.send((color, next, search_time)).unwrap();
            },
            None => {
              // We didn't find a point, put it back in the backfill
              //println!("Search thread {} backfilling color {} with {}", thread_id, color, backfill.len());
              backfill.push(color);
            }
          }
        }
      }).unwrap());
    }

    // Spawn the mutation threads
    for thread_id in 0..num_mutation_threads {
      let rx_mutation_send: Receiver<(Vec<Point>, Vec<Point>)> = rx_mutation_send.clone();
      let tx_mutation_receive = tx_mutation_receive.clone();
      let root = self.root.clone();

      mutation_handles.push(thread::Builder::new().name(format!("Mutator {}", thread_id)).spawn(move || {

        // Our own point pool
        let mut point_pool = vec![Vec::with_capacity(8); 1024];

        loop {
          // Wait for a result to mutate
          let Ok((removals, additions)) = rx_mutation_send.recv() else {
            println!("Mutation thread {} exiting, out of work", thread_id);
            break;
          };

          //println!("Mutation thread {} got {} removals and {} additions", thread_id, removals.len(), additions.len());

          // Remove removals
          let removal_start = Instant::now();
          for removal in removals {
            root.remove(removal, &mut point_pool);
          }
          let removal_duration = removal_start.elapsed().as_micros() as usize;

          // Add additions
          let addition_start = Instant::now();
          for addition in additions {
            root.add(addition, &mut point_pool);
          }
          let addition_duration = addition_start.elapsed().as_micros() as usize;

          tx_mutation_receive.send((removal_duration, addition_duration)).unwrap();
        }
      }).unwrap());
    }

    let mut outstanding = 0;
    let mut color_collisions = Vec::new();

    // Basically just start dispatching work and updating stats
    while self.current_color_idx < pixel_count || outstanding > 0 || !color_collisions.is_empty() {

      // Dispatch a handful of colors
      if outstanding < self.space_mapping.len() + 16 && self.current_color_idx < pixel_count {
        for _ in 0..16 {
          
          // Take either one of our collisions or the next color
          let color = color_collisions.pop().map(|c| {
            trace!("Dispatching {c} from collision list");
            c
          }).unwrap_or_else(|| {
            let color_idx = self.current_color_idx;
            self.current_color_idx += 1;
            let c = self.colors[color_idx];
            trace!("Dispatching {c} from color list");

            // Diagnostics printing
            if self.current_color_idx & 262143 == 0 {
              let i = self.current_color_idx;
              // Progress
              let time_so_far = wall_start_time.elapsed().as_micros();
              let time_per_px = time_so_far as f64 / i as f64;
              let remaining = time_per_px * (4096 * 4096 - i) as f64;

              println!("Adding pixel {i} ({:.1}%), wf = {}, s={}, p={}, r={}, add={}, mr={} mw={}, ETA={:.2}/{:.2}s as {:.2} kpx/s",
                100.0 * (i as f64) / 4096.0 / 4096.0,
                self.root.len(),
                search_time_src / 1000,
                place_time_src / 1000,
                remove_time_src / 1000,
                add_time_src / 1000,
                color_misses_src,
                collision_misses_src,
                remaining / 1000.0 / 1000.0,
                (remaining + time_so_far as f64) / 1000.0 / 1000.0,
                1000.0 / time_per_px,
              );

              // A horrible idea
              let mut scanned_leaves = 0;
              let mut min_leaf_entries = 0;
              let mut max_leaf_entries = 0;
              let mut total_leaf_entries = 0;

              // Statistics on how saturated the leaves are
              // let mut q = Vec::new();
              // q.push(self.root.as_ref());

              // while let Some(next) = q.pop() {
              //   match next {
              //     OctreeLeafy::Node { children, .. } => q.extend_from_slice(children.iter().map(Box::as_ref).collect::<Vec<_>>().as_slice()),
              //     OctreeLeafy::Leaf { points, .. } => {
              //       scanned_leaves += 1;
              //       let len = points.read().len();
              //       min_leaf_entries = min_leaf_entries.min(len);
              //       max_leaf_entries = max_leaf_entries.max(len);
              //       total_leaf_entries += len;
              //     }
              //   }
              // }

              // println!("  Scanned {} leaves, min={}, max={}, avg={}", scanned_leaves, min_leaf_entries, max_leaf_entries, total_leaf_entries / scanned_leaves);
            }

            c
          });

          tx_search_send.send(color).unwrap();
          outstanding += 1;

          if self.current_color_idx >= pixel_count {
            // Reached the end in this batch
            break;
          }

          
        }
      } else if !color_collisions.is_empty() && outstanding < self.space_mapping.len() {
        // Oops gotta dispatch these still
        for _ in 0..16 {
          if let Some(color) = color_collisions.pop() {
            tx_search_send.send(color).unwrap();
            outstanding += 1;
          }
        }
      }

      // Get any search results and verify they can be used
      for (color, result, search_time) in rx_search_receive.try_iter() {
        outstanding -= 1;

        trace!("  Search found {result} for {color}");

        if self.writing_spaces.view_bits::<Msb0>()[result.space().0 as usize] {
          // Already written
          // TODO Need to re-dispatch this somehow
          trace!("    Color {} collided at {} with {} total collisions", color, result.space(), color_collisions.len());
          collision_misses_src += 1;
          // if collision_misses_src > 1000 {
          //   println!("Have {} in the space mapping here", self.space_mapping.get(&result.space()).map(|m| m.len()).unwrap_or(0));

          //   panic!("Too many collisions");
          // }
          color_collisions.push(color);
          continue;
        }

        // Mark as writing
        self.writing_spaces.view_bits_mut::<Msb0>().set(result.space().0 as usize, true);

        // Paint it
        let start = Instant::now();
        self.image.write(&result.space(), &color);
        let paint_duration = start.elapsed().as_micros() as usize;
        place_time_src += paint_duration;

        // Dispatch the result to a mutation thread
        // TODO should we batch these up?
        let to_remove = self.space_mapping.remove(&result.space()).expect("Should have a found point in our global mapping");
        let removals = to_remove.iter().map(|color| Point::new(*result.space(), *color)).collect();

        let mut additions = vec![];
        result.space().get_neighbors(&mut additions);
        // Attach to the color we placed
        // XXX is that right?
        // TODO also probably avoid these rematerializations?
        let additions: Vec<_> = additions
          .iter()
          .filter(|space| !self.writing_spaces.view_bits::<Msb0>()[space.0 as usize])
          .map(|space| Point::new(*space, color))
          .collect();

        // Keep track in our space->color map
        for addition in &additions {
          self.space_mapping.entry(*addition.space()).or_insert(Vec::new()).push(*addition.color());
        }

        for r in &removals { trace!("    Removing {r} because we found {result}"); }
        for a in &additions { trace!("    Adding {a} because it is next to {result}"); }

        tx_mutation_send.send((removals, additions)).unwrap();

        // Update stats
        search_time_src += search_time;
      }

      // Get any mutation results and update stats
      for (removal_time, addition_time) in rx_mutation_receive.try_iter() {
        remove_time_src += removal_time;
        add_time_src += addition_time;
      }

      
    }

    // Drop our send handles to signal the gig is up
    drop(tx_search_send);
    drop(tx_mutation_send);

    // Wait for everybody to finish
    for handle in search_handles {
      handle.join().unwrap();
    }

    for handle in mutation_handles {
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
  let mut spaces = Box::new(vec![ZERO; 4096*4096]);

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