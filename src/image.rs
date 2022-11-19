use std::sync::atomic::{AtomicU8, Ordering};

use crate::{atomicbitmask::AtomicBitMask, points::{SpacePoint, ColorPoint}};

pub struct Image {
    r: Box<Vec<AtomicU8>>,
    g: Box<Vec<AtomicU8>>,
    b: Box<Vec<AtomicU8>>,
    written: AtomicBitMask,
}

impl Image {
    pub fn new() -> Image {
        Image {
            r: box std::iter::repeat_with(AtomicU8::default).take(4096*4096).collect(),
            g: box std::iter::repeat_with(AtomicU8::default).take(4096*4096).collect(),
            b: box std::iter::repeat_with(AtomicU8::default).take(4096*4096).collect(),
            written: AtomicBitMask::new(4096*4096)
        }
    }

    pub fn write(&self, space: &SpacePoint, color: &ColorPoint) {
        //println!("wrote {}", space.offset());
        let offset = space.offset();
        let was_written = self.written.test_and_set(offset);
        assert!(!was_written, "double write");
        self.r[offset].store(color.r, Ordering::Relaxed);
        self.g[offset].store(color.g, Ordering::Relaxed);
        self.b[offset].store(color.b, Ordering::Relaxed);
    }

    pub fn to_raw(&self) -> Box<[u8; 4096*4096*4]> {
        let mut ret = box [0u8; 4096*4096*4];
        for y in 0..4096 {
            for x in 0..4096 {
                let o = usize::try_from(y << 12 | x).expect("Should not index a point beyond 2^24");

                ret[4 * o    ] = self.r[o].load(Ordering::Relaxed);
                ret[4 * o + 1] = self.g[o].load(Ordering::Relaxed);
                ret[4 * o + 2] = self.b[o].load(Ordering::Relaxed);
                ret[4 * o + 3] = 255;
            }
        }

        ret
    }

    pub fn has(&self, position: usize) -> bool {
        self.written.test(position)
    }
}