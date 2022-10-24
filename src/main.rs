

use rust_colors::{points, bounding_box};


fn main() {
    let foo = points::Point {
        space: points::SpacePoint{ x: 1, y: 2, written: false },
        color: points::ColorPoint{ r: 6, g: 88, b: 128 },
        idx: 14
    };

    println!("{foo}");

    let bar = bounding_box::BoundingBox::from_around(&foo.color, 15);
    println!("{bar}");

    

    println!("All done");
}
