use std::{sync::{atomic::{AtomicU8, Ordering, AtomicU64}}, fs::File, io::Write};

// Atomic type holding the slot values
type AtomicType = AtomicU64;
// Primitive type stored in the slots
type InnerType = u64;
// Size of an in-slot address
const INNER_SHIFT: usize = 6;
// Number of bits in a slot
const INNER_BITS: usize = 1 << INNER_SHIFT;
// Mask for position to get in-slot address
const INNER_MASK: usize = INNER_BITS - 1;

pub struct AtomicBitMask {
    bits: Box<Vec<AtomicType>>,
}

impl AtomicBitMask {
    pub fn new(size: usize) -> AtomicBitMask {
        AtomicBitMask {
            bits: box std::iter::repeat_with(|| AtomicType::new(0))
                .take((size + INNER_BITS - 1)/INNER_BITS) // XXX a more clear way to allocate enough octets?
                .collect::<Vec<_>>()
        }
    }

    fn get_mask(&self, position: usize) -> InnerType {
        1 << (position & INNER_MASK)
    }

    fn get_slot(&self, position: usize) -> &AtomicType {
        &self.bits[position >> INNER_SHIFT]
    }

    /// Sets the specified bit, and returns if it was previously set
    pub fn test_and_set(&self, position: usize) -> bool {
        let mask = self.get_mask(position);
        let last_octet = self.get_slot(position)
            .fetch_or(mask, Ordering::SeqCst);
        last_octet & mask != 0
    }

    pub fn test(&self, position: usize) -> bool {
        self.get_slot(position).load(Ordering::SeqCst) & self.get_mask(position) != 0
    }

    pub fn clear(&self, position: usize) -> bool {
        let mask = self.get_mask(position);
        let last_octet = self.get_slot(position)
            .fetch_and(!mask, Ordering::SeqCst);
        last_octet & mask != 0
    }

    pub fn diag(&self, mut out: File) {
        for octet in self.bits.iter() {
            out.write_fmt(format_args!("{:08b}", octet.load(Ordering::Relaxed))).unwrap();
        }
    }

    pub fn iter_set(&self) -> AtomicBitMaskIter<'_> {
        AtomicBitMaskIter {
            mask: self,
            current_slot_idx: 0,
            current_value: self.bits[0].load(Ordering::Relaxed),
            //current_bit: 0,
            finished: false,
        }
    }
    
}


pub struct AtomicBitMaskIter<'a> {
    pub mask: &'a AtomicBitMask,
    current_slot_idx: usize,
    current_value: InnerType,
    finished: bool,
}

impl<'a> Iterator for AtomicBitMaskIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            // Stop fast if we already finished
            return None;
        }

        'outer:
        loop {

            let lz = self.current_value.trailing_zeros();
            if lz < 64 {
                // We should have a bit at the given index
                assert!(self.current_value & (1 << lz) != 0);
                // Clear that bit for next time
                //self.current_value &= !(1 << lz);
                self.current_value ^= 1 << lz;
                // Return the slot index added to the found index
                return Some((self.current_slot_idx << INNER_SHIFT) | lz as usize);
            }

            // No bits set in that slot, move up
            self.current_slot_idx += 1;

            // Load more while we can, until we find a non-zero entry
            while self.current_slot_idx < self.mask.bits.len() {
                self.current_value = self.mask.bits[self.current_slot_idx].load(Ordering::Relaxed);

                if self.current_value == 0 {
                    // If there is nothing set, just skip it right away
                    self.current_slot_idx += 1;
                } else {
                    continue 'outer;
                }
            }
            
            // Nothing more to load, stop iterating
            self.finished = true;
            return None;
        }
    }
}

#[test]
fn basic_test() {
    let num_bits = 33;

    let mask = AtomicBitMask::new(num_bits);
    for i in 0..num_bits {
        assert!(!mask.test_and_set(i), "Bit was set before write");
    }

    for i in 0..num_bits {
        assert!(mask.test(i), "Bit was unset after write");
    }

    for i in 0..num_bits {
        assert!(mask.test_and_set(i), "Bit was set before write");
    }
}

#[test]
fn threaded_test() {
    use std::sync::Arc;
    use core::sync::atomic::AtomicUsize;
    use std::thread;

    let num_bits = 4096;
    let thread_count = 16;
    
    let queue_mask = Arc::new(AtomicBitMask::new(num_bits));
    let written_mask = Arc::new(AtomicBitMask::new(num_bits));
    let next_index = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let my_queue_mask = Arc::clone(&queue_mask);
        let my_written_mask = Arc::clone(&written_mask);
        let my_next_index = Arc::clone(&next_index);

        let handle = thread::Builder::new().name(format!("Test Thread {thread_id}")).spawn(move || {
            loop {
                let next_index = my_next_index.fetch_add(1, Ordering::Relaxed);

                if next_index >= num_bits {
                    // Got to the end
                    break;
                }

                println!("Test Thread {thread_id} is writing {next_index}");
                
                // We should only write to written_mask once
                if !my_queue_mask.test_and_set(next_index) {
                    assert!(!my_written_mask.test_and_set(next_index))
                }
            }
        });

        handles.push(handle.unwrap());
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // We should have written to everything
    for i in 0..num_bits {
        assert!(queue_mask.test(i), "Missed queue mask");
        assert!(written_mask.test(i), "Missed written mask");
    }
}

#[test]
fn iter_set_test() {
    let mask = AtomicBitMask::new(128);

    mask.test_and_set(5);
    mask.test_and_set(7);
    mask.test_and_set(19);
    mask.test_and_set(63);
    mask.test_and_set(64);
    mask.test_and_set(65);

    assert!(mask.test(5));
    assert!(mask.test(7));
    assert!(mask.test(19));
    assert!(mask.test(63));
    assert!(mask.test(64));
    assert!(mask.test(65));

    let set = mask.iter_set().collect::<Vec<_>>();

    assert_eq!(set.len(), 6);
    assert_eq!(set[0], 5);
    assert_eq!(set[1], 7);
    assert_eq!(set[2], 19);
    assert_eq!(set[3], 63);
    assert_eq!(set[4], 64);
    assert_eq!(set[5], 65);
    
}