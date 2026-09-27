//! Dependency-free deterministic mutation driver. No libfuzzer-sys / FFI.
#![forbid(unsafe_code)]
fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
pub fn run(seeds: &[&[u8]], mut target: impl FnMut(&[u8])) {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(10000);
    let mut state = std::env::args()
        .nth(2)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(0x12345678)
        .max(1);
    for &s in seeds {
        target(s);
    }
    for i in 0..iterations {
        let mut b = if i % 3 == 0 {
            let n = (random(&mut state) % 4096) as usize;
            let mut b = vec![0; n];
            for x in &mut b {
                *x = random(&mut state) as u8;
            }
            b
        } else {
            seeds[i % seeds.len()].to_vec()
        };
        for _ in 0..1 + random(&mut state) % 8 {
            if b.is_empty() {
                b.push(random(&mut state) as u8);
                continue;
            }
            let index = (random(&mut state) as usize) % b.len();
            match random(&mut state) % 5 {
                0 => b.truncate(index),
                1 => b[index] ^= 1 << ((random(&mut state) % 8) as u32),
                2 => b[index] = 0xff,
                3 => b[index] = 0,
                _ => b.push(random(&mut state) as u8),
            }
        }
        target(&b);
    }
    println!("completed {iterations} mutations without panic");
}
