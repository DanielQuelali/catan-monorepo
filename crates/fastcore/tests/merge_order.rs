use fastcore::stats::{merge_worker_stats, Stats};

#[test]
fn merge_worker_stats_sorts_by_worker_id() {
    let mut a = Stats::default();
    a.games = 1;
    a.turns = 2;
    a.wins[0] = 1;

    let mut b = Stats::default();
    b.games = 1;
    b.turns = 4;
    b.wins[1] = 1;

    let mut c = Stats::default();
    c.games = 1;
    c.turns = 8;
    c.wins[2] = 1;

    let merged = merge_worker_stats(vec![(2, c.clone()), (0, a.clone()), (1, b.clone())]);

    let mut expected = Stats::default();
    expected.merge(&a);
    expected.merge(&b);
    expected.merge(&c);

    assert_eq!(merged.games, expected.games);
    assert_eq!(merged.turns, expected.turns);
    assert_eq!(merged.wins, expected.wins);
    assert_eq!(merged.illegal_actions, expected.illegal_actions);
}
