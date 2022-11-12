use std::cell::RefCell;
use std::time::Instant;
use std::{path::Path, rc::Rc};
use std::fs::File;
use std::io::BufWriter;
use crate::{points::{ColorPoint, SpacePoint, Point}, octree::Octree, bounding_box::BoundingBox};
use bitvec::prelude::*;

type ImageType = Box<[u8; 4096*4096*4]>;
type SpacePoints = Box<[SpacePoint; 4096*4096]>;

pub struct ColorGenerator {
  colors: Vec<Rc<ColorPoint>>,
  spaces: SpacePoints,
  written_spaces: RefCell<Box<BitArr!(for 4096*4096)>>,
  root: Rc<Octree>,
  image: ImageType,
  current_color_idx: RefCell<usize>,
}

// Public things
impl ColorGenerator {
  pub fn new() -> Box<ColorGenerator> {
    box ColorGenerator {
      colors: initialize_color_space(),
      spaces: initialize_space_space(),
      current_color_idx: RefCell::new(0),
      image: box [0; 4096*4096*4],
      written_spaces: RefCell::new(box bitarr![usize, Lsb0; 0; 4096*4096]),
      root: Octree::new(None, 0, 0, BoundingBox::new(0, 0, 0, 256, 256, 256)),
    }
  }

  pub fn shuffle_colors(&mut self) {
    println!("Doing the color shuffle");
    //let mut rng = rand::thread_rng();
    //self.colors.shuffle(&mut rng);
    fastrand::shuffle(&mut self.colors);
    
  }

  pub fn add_next_seed_pixel(&mut self, x: u32, y: u32) {
    let color = &self.colors[self.current_color_idx.borrow().to_owned()];
    *self.current_color_idx.borrow_mut() += 1;
    let ofs = space_offset(x, y);

    if self.written_spaces.borrow_mut()[ofs] {
      panic!("Seeded already written point");
    }

    let space = &self.spaces[ofs];

    Self::place_pixel(&mut self.image, space, color);

    self.add_neighbors(space, color);

    self.root.add(Rc::new(Point { color: Rc::clone(color), space: ofs }));
    
  }

  fn add_neighbors(&self, space: &SpacePoint, color: &Rc<ColorPoint>) {
    for neighbor in space.get_neighbors() {
      if self.written_spaces.borrow()[neighbor] {
        continue;
      } else {
        let new_point: Rc<Point> = Rc::new(Point {
          color: Rc::clone(color),
          space: neighbor
        });


        self.root.add(new_point);
      }
    }
  }

  // pub fn add_specific_seed_pixel(&self, x: i32, y: i32, r: u8, g: u8, b: u8) {
  //   todo!("Add specific seed pizels");
  // }

  pub fn grow_pixels_to(&mut self, pixel_count: usize) {
    
    if self.current_color_idx.borrow().to_owned() == 0 {
      panic!("Tried to call grow_pixels_to without any seed pixels");
    }

    let mut search_time = 0;
    let mut place_time = 0;
    let mut remove_time = 0;
    let mut add_time = 0;
    
    for i in self.current_color_idx.borrow().to_owned()..pixel_count {
      let mut start = Instant::now();
      if i & 262143 == 0 {
        // Progress
        let time_so_far = search_time + place_time + remove_time + add_time;
        let time_per_px = time_so_far as f64 / i as f64;
        let remaining = time_per_px * (4096 * 4096 - i) as f64;

        println!("Adding pixel {i} ({:.1}%), wf = {}, s={}, p={}, r={}, add={}, ETA={:.2}/{:.2}s as {:.2} kpx/s",
          100.0 * (i as f64) / 4096.0 / 4096.0,
          self.root.len(),
          search_time / 1000,
          place_time / 1000,
          remove_time / 1000,
          add_time / 1000,
          remaining / 1000.0 / 1000.0,
          (remaining + time_so_far as f64) / 1000.0 / 1000.0,
          1000.0 / time_per_px
        );

        
      }
      
      let at = &self.colors[i];
      let next = self.root.find_nearest(at).expect("Tried to add a pixel but there were none to grow on");

      search_time += start.elapsed().as_micros();
      start = Instant::now();

      let space = &self.spaces[next.space];

      // Mark done
      self.written_spaces.borrow_mut().set(space_offset(space.x, space.y), true);

      //println!("  It was {at} at {space}, wf={}", self.root.len());

      Self::place_pixel(&mut self.image, space, at);

      place_time += start.elapsed().as_micros();
      start = Instant::now();

      // Remove that one
      self.root.remove(&next);

      remove_time += start.elapsed().as_micros();
      start = Instant::now();

      // Add new ones
      self.add_neighbors(space, at);

      add_time += start.elapsed().as_micros();
    }
  }

  pub fn place_pixel(image: &mut ImageType, space: &SpacePoint, color: &ColorPoint) {
    let idx = space_offset(space.x, space.y) * 4;
    
    image[idx    ] = color.r;
    image[idx + 1] = color.g;
    image[idx + 2] = color.b;
    image[idx + 3] = 255;
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

    writer.write_image_data(self.image.as_ref()).unwrap(); // Save
  }
}




/// Sets up our list of colors and color pointers
fn initialize_color_space() -> Vec<Rc<ColorPoint>> {
  let mut colors = Vec::with_capacity(4096 * 4096);
  
  for r in 0..=255u8 {
    for g in 0..=255u8 {
      for b in 0..=255u8 {
        //let idx = usize::from(r) << 16 | usize::from(g) << 8 | usize::from(b);
        //self.color_space[idx] = &self.colors[idx];

        colors.push(Rc::new(ColorPoint { r, g, b }));
      }
    }
  }

  colors
}

/// Sets up our list of points
fn initialize_space_space() -> SpacePoints {
  let now = Instant::now();
  
  const ZERO: SpacePoint = SpacePoint { x: 0, y: 0 };
  let mut spaces = box [ZERO; 4096*4096];

  println!("  Alloc spaces in {}", now.elapsed().as_millis());

  for x in 0..4096u32 {
    for y in 0..4096u32 {
      let mut point = &mut spaces[space_offset(x, y)];

      point.x = x;
      point.y = y;
    }
  }

  println!("  Init spaces in {}", now.elapsed().as_millis());


  spaces//.map(|s| Rc::new(s))
}

// Helpers

/// Converts an x/y coordinate to an index
fn space_offset(x: u32, y: u32) -> usize {
  usize::try_from(y << 12 | x).expect("Should not index a point beyond 2^24")
}