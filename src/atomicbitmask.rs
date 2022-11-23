use std::{sync::{atomic::{AtomicU8, Ordering}}, fs::File, io::Write};

pub struct AtomicBitMask {
    bits: Box<Vec<AtomicU8>>,
}

impl AtomicBitMask {
    pub fn new(size: usize) -> AtomicBitMask {
        AtomicBitMask {
            bits: box std::iter::repeat_with(|| AtomicU8::new(0))
                .take((size + 7)/8) // XXX a more clear way to allocate enough octets?
                .collect::<Vec<_>>()
        }
    }

    fn get_mask(&self, position: usize) -> u8 {
        1 << (position & 0x7)
    }

    fn get_octet(&self, position: usize) -> &AtomicU8 {
        &self.bits[position >> 3]
    }

    /// Sets the specified bit, and returns if it was previously set
    pub fn test_and_set(&self, position: usize) -> bool {
        let mask = self.get_mask(position);
        let last_octet = self.get_octet(position)
            .fetch_or(mask, Ordering::SeqCst);
        last_octet & mask != 0
    }

    pub fn test(&self, position: usize) -> bool {
        self.get_octet(position).load(Ordering::SeqCst) & self.get_mask(position) != 0
    }

    pub fn clear(&self, position: usize) -> bool {
        let mask = self.get_mask(position);
        let last_octet = self.get_octet(position)
            .fetch_and(!mask, Ordering::SeqCst);
        last_octet & mask != 0
    }

    pub fn diag(&self, mut out: File) {
        for octet in self.bits.iter() {
            out.write_fmt(format_args!("{:08b}", octet.load(Ordering::Relaxed))).unwrap();
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

