use rand_core::{Error, RngCore};

#[derive(Clone, Debug)]
pub struct Pcg64 {
    state: u128,
    increment: u128,
}

impl Pcg64 {
    const MULTIPLIER: u128 = 0x2360ED051FC65DA44385DF649FCCF645;

    pub fn from_seed(base_seed: u64, stream: u64) -> Self {
        let mut state = (base_seed as u128) | ((stream as u128) << 64);
        let increment = 1u128;
        state = state.wrapping_add(increment);
        let mut rng = Self { state, increment };
        rng.step();
        rng
    }

    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(self.increment);
    }

    fn output_xsl_rr(state: u128) -> u64 {
        let rot = ((state >> 122) & 0x3f) as u32;
        let xsl = ((state >> 64) ^ state) as u64;
        xsl.rotate_right(rot)
    }
}

impl RngCore for Pcg64 {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.step();
        Self::output_xsl_rr(self.state)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut idx = 0;
        while idx < dest.len() {
            let value = self.next_u64().to_le_bytes();
            let take = (dest.len() - idx).min(value.len());
            dest[idx..idx + take].copy_from_slice(&value[..take]);
            idx += take;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

pub fn rng_for_stream(base_seed: u64, stream: u64) -> Pcg64 {
    Pcg64::from_seed(base_seed, stream)
}

pub fn next_u64_mod(rng: &mut impl RngCore, modulus: u64) -> u64 {
    if modulus == 0 {
        return 0;
    }
    rng.next_u64() % modulus
}

pub fn shuffle_with_rng<T>(items: &mut [T], rng: &mut impl RngCore) {
    if items.len() < 2 {
        return;
    }
    for i in (1..items.len()).rev() {
        let j = next_u64_mod(rng, (i + 1) as u64) as usize;
        items.swap(i, j);
    }
}

pub fn roll_die(rng: &mut impl RngCore) -> u8 {
    (next_u64_mod(rng, 6) + 1) as u8
}
