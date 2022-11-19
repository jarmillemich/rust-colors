#![feature(box_syntax)]

use std::{time::Instant, mem, sync::{atomic::AtomicU8, Arc, RwLock}};

use rust_colors::{color_generator::ColorGenerator};


fn main() {
    let start = Instant::now();

    println!("{}", mem::size_of::<AtomicU8>());
    
    println!("Starting");

    let whatever = Arc::new(RwLock::new(ColorGenerator::new()));

    let elapsed = start.elapsed();
    println!("Init Generator at {}", elapsed.as_millis());

    whatever.write().unwrap().shuffle_colors();
    let elapsed = start.elapsed();
    println!("Shuffle at {}", elapsed.as_millis());

    whatever.read().unwrap().add_next_seed_pixel(1024, 1024);
    let elapsed = start.elapsed();
    println!("Add seed at {}", elapsed.as_millis());

    

    ColorGenerator::grow_pixels_to(&whatever, 4096*4096);
    let elapsed = start.elapsed();
    println!("Grown at {}", elapsed.as_millis());

    whatever.read().unwrap().write_image(&String::from("./test.png"));
    let elapsed = start.elapsed();
    println!("Wrote at {}", elapsed.as_millis());

    println!("All done");
}
