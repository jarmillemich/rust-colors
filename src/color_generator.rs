use std::path::Path;
use std::fs::File;
use std::io::BufWriter;
use rand::Rng;
use crate::{points::{ColorPoint, SpacePoint}, octree::Octree, bounding_box::BoundingBox};

pub struct ColorGenerator<'a> {
  colors: [ColorPoint; 4096*4096],
  //color_space: [&'a ColorPoint; 4096*4096],
  spaces: [SpacePoint; 4096*4096],
  root: Octree<'a>,
  image: [u8; 4096*4096*4],
  current_color_idx: usize,
}

// Public things
impl<'a> ColorGenerator<'a> {
  pub fn new() -> ColorGenerator<'a> {
    let colors = initialize_color_space();
    
    let ret = ColorGenerator {
      colors,
      spaces: initialize_space_space(),
      current_color_idx: 0,
      image: [0u8; 4096*4096*4],
      root: Octree::new(None, 0, 0, BoundingBox::new(0, 0, 0, 256, 256, 256)),
      //color_space: colors.iter().enumerate().map(|(i, _)| &colors[i]).collect::<Vec<&'a ColorPoint>>().try_into().unwrap()
    };

    ret
  }

  pub fn shuffle_colors(&mut self) {
    println!("Doing the color shuffle");
    let mut rng = rand::thread_rng();

    for i in 0..4096*4096-1 {
      let j = rng.gen_range(i..4096*4096);
      self.colors.swap(i, j);
    }
  }

  pub fn add_next_seed_pixel(&self, x: i32, y: i32) {

  }

  pub fn add_specific_seed_pixel(&self, x: i32, y: i32, r: u8, g: u8, b: u8) {

  }

  pub fn grow_pixels_to(&mut self, pixel_count: usize) {
    
    if self.current_color_idx == 0 {
      panic!("Tried to call grow_pixels_to without any seed pixels");
    }
    
    for i in self.current_color_idx..pixel_count {
      println!("Adding pixel {i}");

      let at = &self.colors[i];
      let next = self.root.find_nearest(at).expect("Tried to add a pixel but there were none to grow on");

      Self::place_pixel(&mut self.image, &next.space, at);
    }
  }

  pub fn place_pixel(image: &mut [u8; 4096*4096*4], space: &SpacePoint, color: &ColorPoint) {
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

    writer.write_image_data(&self.image).unwrap(); // Save
  }
}




/// Sets up our list of colors and color pointers
fn initialize_color_space() -> [ColorPoint; 4096*4096] {
  let mut colors = [ColorPoint { r: 0, g: 0, b: 0}; 4096*4096];
  
  for r in 0..=255u8 {
    for g in 0..=255u8 {
      for b in 0..=255u8 {
        let idx = usize::from(r) << 16 | usize::from(g) << 8 | usize::from(b);
        //self.color_space[idx] = &self.colors[idx];

        colors[idx].r = r;
        colors[idx].g = g;
        colors[idx].b = b;
      }
    }
  }

  colors
}

/// Sets up our list of points
fn initialize_space_space() -> [SpacePoint; 4096*4096] {
  let mut spaces = [SpacePoint::zero(); 4096*4096];
  
  for x in 0..4096 {
    for y in 0..4096 {
      let mut point = &mut spaces[space_offset(x, y)];

      point.x = x;
      point.y = y;
      // TODO what was this for?
      //point.hash = space_offset(x, y);
    }
  }

  spaces
}

// Helpers

/// Converts an x/y coordinate to an index
fn space_offset(x: u32, y: u32) -> usize {
  usize::try_from(y << 12 | x).expect("Should not index a point beyond 2^24")
}