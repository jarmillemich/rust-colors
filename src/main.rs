#![feature(box_syntax)]

use std::{time::Instant};

use rust_colors::{color_generator::ColorGenerator};


fn main() {
    let start = Instant::now();
    
    println!("Starting");

    let mut whatever = box ColorGenerator::new();

    let elapsed = start.elapsed();
    println!("Init Generator at {}", elapsed.as_millis());

    whatever.shuffle_colors();
    let elapsed = start.elapsed();
    println!("Shuffle at {}", elapsed.as_millis());

    whatever.add_next_seed_pixel(1024, 1024);
    let elapsed = start.elapsed();
    println!("Add seed at {}", elapsed.as_millis());

    whatever.write_image(&String::from("./test.png"));
    let elapsed = start.elapsed();
    println!("Wrote at {}", elapsed.as_millis());

    whatever.grow_pixels_to(1000);
    let elapsed = start.elapsed();
    println!("Grown at {}", elapsed.as_millis());

    println!("All done");
}
