use std::cell::RefCell;
use std::time::Instant;
use std::{path::Path, rc::Rc};
use std::fs::File;
use std::io::BufWriter;
use crate::points::Point;
use crate::{points::{ColorPoint, SpacePoint}, octree::Octree, bounding_box::BoundingBox};

type ImageType = Box<[u8; 4096*4096*4]>;
type SpacePoints = Box<[Rc<SpacePoint>; 4096*4096]>;

pub struct ColorGenerator {
  colors: Vec<Rc<ColorPoint>>,
  //color_space: [&'a ColorPoint; 4096*4096],
  spaces: SpacePoints,
  written_spaces: Box<[bool; 4096*4096]>,
  root: Rc<Octree>,
  //image: Vec<u8>,
  image: ImageType,
  current_color_idx: RefCell<usize>,
}

// Public things
impl ColorGenerator {
  pub fn new() -> Box<ColorGenerator> {
    let colors = initialize_color_space();
    // let mut image = Vec::with_capacity(4096*4096*4);
    // for i in 0..4096*4096*4 { image.push(0); }
    
    let ret = box ColorGenerator {
      colors,
      spaces: initialize_space_space(),
      current_color_idx: RefCell::new(0),
      image: box [0; 4096*4096*4],
      written_spaces: box [false; 4096*4096],
      root: Octree::new(None, 0, 0, BoundingBox::new(0, 0, 0, 256, 256, 256)),
      //color_space: colors.iter().enumerate().map(|(i, _)| &colors[i]).collect::<Vec<&'a ColorPoint>>().try_into().unwrap(),
    };

    ret
  }

  pub fn shuffle_colors(&mut self) {
    println!("Doing the color shuffle");
    //let mut rng = rand::thread_rng();
    //self.colors.shuffle(&mut rng);
    fastrand::shuffle(&mut self.colors);
    
  }

  pub fn add_next_seed_pixel(&self, x: u32, y: u32) {
    let color = &self.colors[self.current_color_idx.borrow().to_owned()];
    *self.current_color_idx.borrow_mut() += 1;
    let ofs = space_offset(x, y);

    if self.written_spaces[ofs] {
      panic!("Seeded already written point");
    }

    let space = &self.spaces[ofs];

    for neighbor in space.get_neighbors() {
      if self.written_spaces[neighbor] {
        continue;
      } else {
        let new_point: Rc<Point> = Rc::new(Point {
          color: Rc::clone(color),
          space: Rc::clone(&self.spaces[neighbor])
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
    
    for i in self.current_color_idx.borrow().to_owned()..pixel_count {
      println!("Adding pixel {i}");

      let at = &self.colors[i];
      let next = self.root.find_nearest(&at).expect("Tried to add a pixel but there were none to grow on");

      Self::place_pixel(&mut self.image, &next.space, &at);
    }
  }

  pub fn place_pixel(image: &mut ImageType, space: &SpacePoint, color: &ColorPoint) {
    let idx = space_offset(space.x, space.y) * 4;
    
    image[idx + 0] = color.r;
    image[idx + 1] = color.g;
    image[idx + 2] = color.b;
    image[idx + 3] = 255;
  }

  pub fn write_image(&self, path_spec: &String) {
    let path = Path::new(path_spec);
    let file = File::create(path).unwrap();
    let ref mut w = BufWriter::new(file);

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

  box spaces.map(|s| Rc::new(s))
}

// Helpers

/// Converts an x/y coordinate to an index
fn space_offset(x: u32, y: u32) -> usize {
  usize::try_from(y << 12 | x).expect("Should not index a point beyond 2^24")
}