use fastcore::rng::rng_for_stream;
use rand_core::RngCore;

#[test]
fn rng_is_deterministic_for_same_seed() {
    let mut a = rng_for_stream(1234, 0);
    let mut b = rng_for_stream(1234, 0);

    for _ in 0..16 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn rng_diverges_for_different_streams() {
    let mut a = rng_for_stream(1234, 0);
    let mut b = rng_for_stream(1234, 1);

    let mut any_diff = false;
    for _ in 0..16 {
        if a.next_u64() != b.next_u64() {
            any_diff = true;
            break;
        }
    }

    assert!(any_diff, "streams should diverge");
}
