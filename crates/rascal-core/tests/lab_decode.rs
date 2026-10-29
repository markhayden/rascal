//! Task 6.3: the decode rules on constructed pick sets, the rejections, and the draw. A group
//! is `preferred` when wins over decided pairs reach the protocol's fraction, `not_worse`
//! when losses stay below it, `worse` otherwise, `inconclusive` under `min_decided`; `same`
//! and `neither` count toward neither side; a stale digest, a missing, duplicated, foreign or
//! malformed pick is refused naming it; the assignment's order and sides come from the seed
//! alone.
#![allow(clippy::unwrap_used)]

use rascal_core::lab::pairs::{Assignment, Candidate, Draw, Pair, Pick, Picks, Shown, decode};
use rascal_core::lab::protocol::Protocol;

fn protocol(min_decided: u32) -> Protocol {
    Protocol::parse(
        &format!(
            "version = 1\nname = \"t\"\nseed = 42\narms = [\"a\", \"b\"]\nmin_decided = {min_decided}\n[[group]]\nname = \"sky\"\nimages = [\"s1\", \"s2\", \"s3\", \"s4\", \"s5\", \"s6\"]\n[[group]]\nname = \"structured\"\nimages = [\"t1\", \"t2\", \"t3\"]\n"
        ),
        "t",
    )
    .unwrap()
}

fn shown(cell: &str) -> Shown {
    Shown {
        cell: cell.to_string(),
        attempt: 1,
    }
}

/// An assignment with a known side per pair: arm a on the left for every pair.
fn assignment(p: &Protocol) -> Assignment {
    Assignment {
        assignment_version: 1,
        lab: "t".into(),
        protocol_digest: "sha256:p".into(),
        arms: ["a".into(), "b".into()],
        seed: p.seed,
        pair: p
            .group
            .iter()
            .flat_map(|g| {
                g.images.iter().map(|id| Pair {
                    id: id.clone(),
                    group: g.name.clone(),
                    left: shown(&format!("a-{id}")),
                    right: shown(&format!("b-{id}")),
                    a_side: "left".into(),
                })
            })
            .collect(),
    }
}

fn picks(pairs: &[(&str, &str)]) -> Picks {
    Picks {
        picks_version: 1,
        assignment_digest: "sha256:assignment".into(),
        pick: pairs
            .iter()
            .map(|(pair, pick)| Pick {
                pair: pair.to_string(),
                pick: pick.to_string(),
            })
            .collect(),
    }
}

const ALL: [&str; 9] = ["s1", "s2", "s3", "s4", "s5", "s6", "t1", "t2", "t3"];

fn full(sky: [&str; 6], structured: [&str; 3]) -> Picks {
    let mut list: Vec<(&str, &str)> = Vec::new();
    for (i, id) in ALL.iter().enumerate() {
        list.push((id, if i < 6 { sky[i] } else { structured[i - 6] }));
    }
    picks(&list)
}

#[test]
fn the_outcome_rules_on_constructed_pick_sets() {
    let p = protocol(3);
    let a = assignment(&p);
    let digest = "sha256:assignment";
    // (sky picks, structured picks, expected sky outcome and counts, expected structured).
    // Arm a is on the left of every pair, so "left" is a win and "right" a loss.
    type Counts = (&'static str, u32, u32, u32, u32, u32);
    let cases: Vec<([&str; 6], [&str; 3], Counts, &str)> = vec![
        // 4 wins of 6 decided (2/3, inclusive): preferred. 2 decided under min 3: inconclusive.
        (
            ["left", "left", "left", "left", "right", "right"],
            ["left", "right", "same"],
            ("preferred", 6, 4, 2, 0, 0),
            "inconclusive",
        ),
        // 3 wins, 3 losses: not preferred, losses 1/2 not below 1/3: worse.
        (
            ["left", "left", "left", "right", "right", "right"],
            ["left", "left", "left"],
            ("worse", 6, 3, 3, 0, 0),
            "preferred",
        ),
        // same and neither are not decided: 4 decided, 3 wins (3/4): preferred.
        (
            ["left", "left", "left", "right", "same", "same"],
            ["neither", "neither", "neither"],
            ("preferred", 4, 3, 1, 2, 0),
            "inconclusive",
        ),
        // 5 decided, 3 wins (3/5 < 2/3), 2 losses (2/5 ≥ 1/3): worse.
        (
            ["left", "left", "left", "right", "right", "neither"],
            ["left", "left", "right"],
            ("worse", 5, 3, 2, 0, 1),
            "preferred",
        ),
    ];
    for (sky, structured, (outcome, decided, wins, losses, same, neither), structured_outcome) in
        cases
    {
        let d = decode(&p, &a, digest, &full(sky, structured)).unwrap();
        assert_eq!(d.arms, ["a".to_string(), "b".to_string()]);
        let g = &d.groups[0];
        assert_eq!(
            (
                g.outcome, g.pairs, g.decided, g.wins, g.losses, g.same, g.neither
            ),
            (outcome, 6, decided, wins, losses, same, neither),
            "sky {sky:?}"
        );
        assert_eq!(
            d.groups[1].outcome, structured_outcome,
            "structured {structured:?}"
        );
    }
    // Under the spec's default fractions `not_worse` alone is unreachable: wins and losses
    // sum to the decided pairs, so losses below one third means wins above two thirds, which
    // is `preferred`. Every split of up to twenty decided pairs says so.
    let wide = Protocol::parse(
        "version = 1\nname = \"t\"\nseed = 1\narms = [\"a\", \"b\"]\nmin_decided = 1\n[[group]]\nname = \"g\"\nimages = [\"i1\", \"i2\", \"i3\", \"i4\", \"i5\", \"i6\", \"i7\", \"i8\", \"i9\", \"i10\", \"i11\", \"i12\", \"i13\", \"i14\", \"i15\", \"i16\", \"i17\", \"i18\", \"i19\", \"i20\"]\n",
        "t",
    )
    .unwrap();
    let wa = assignment(&wide);
    let ids: Vec<String> = (1..=20).map(|i| format!("i{i}")).collect();
    for wins in 0..=20u32 {
        for losses in 0..=(20 - wins) {
            let list: Vec<(&str, &str)> = ids
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let i = i as u32;
                    (
                        id.as_str(),
                        if i < wins {
                            "left"
                        } else if i < wins + losses {
                            "right"
                        } else {
                            "same"
                        },
                    )
                })
                .collect();
            let d = decode(&wide, &wa, digest, &picks(&list)).unwrap();
            let outcome = d.groups[0].outcome;
            assert!(
                ["preferred", "worse", "inconclusive"].contains(&outcome),
                "{wins} wins, {losses} losses: {outcome}"
            );
        }
    }
    // A protocol whose fractions leave a gap can report not_worse: 7 decided, 5 wins (5/7
    // below 3/4), 2 losses (2/7 below 1/3).
    let gap = Protocol::parse(
        "version = 1\nname = \"t\"\nseed = 1\narms = [\"a\", \"b\"]\nmin_decided = 3\n[rules]\npreferred = [3, 4]\nnot_worse = [1, 3]\n[[group]]\nname = \"g\"\nimages = [\"i1\", \"i2\", \"i3\", \"i4\", \"i5\", \"i6\", \"i7\"]\n",
        "t",
    )
    .unwrap();
    let ga = assignment(&gap);
    let d = decode(
        &gap,
        &ga,
        digest,
        &picks(&[
            ("i1", "left"),
            ("i2", "left"),
            ("i3", "left"),
            ("i4", "left"),
            ("i5", "left"),
            ("i6", "right"),
            ("i7", "right"),
        ]),
    )
    .unwrap();
    assert_eq!(d.groups[0].outcome, "not_worse");
}

#[test]
fn stale_missing_duplicated_foreign_and_malformed_picks_are_refused_by_name() {
    let p = protocol(1);
    let a = assignment(&p);
    let sound = full(["left"; 6], ["left"; 3]);
    decode(&p, &a, "sha256:assignment", &sound).unwrap();
    let err = decode(&p, &a, "sha256:other", &sound).unwrap_err();
    assert!(err.contains("stale or foreign"), "{err}");
    let mut missing = sound.clone();
    missing.pick.retain(|k| k.pair != "s3");
    let err = decode(&p, &a, "sha256:assignment", &missing).unwrap_err();
    assert!(err.contains("pair s3 has no pick"), "{err}");
    let mut duplicated = sound.clone();
    duplicated.pick.push(Pick {
        pair: "s3".into(),
        pick: "right".into(),
    });
    let err = decode(&p, &a, "sha256:assignment", &duplicated).unwrap_err();
    assert!(err.contains("pair s3 is picked twice"), "{err}");
    let mut foreign = sound.clone();
    foreign.pick.push(Pick {
        pair: "zz".into(),
        pick: "left".into(),
    });
    let err = decode(&p, &a, "sha256:assignment", &foreign).unwrap_err();
    assert!(err.contains("pair zz is not in the assignment"), "{err}");
    let mut malformed = sound.clone();
    malformed.pick[0].pick = "both".into();
    let err = decode(&p, &a, "sha256:assignment", &malformed).unwrap_err();
    assert!(err.contains("pair s1: pick \"both\""), "{err}");
    let mut version = sound;
    version.picks_version = 2;
    let err = decode(&p, &a, "sha256:assignment", &version).unwrap_err();
    assert!(err.contains("picks_version 2"), "{err}");
}

#[test]
fn the_draw_is_a_function_of_the_seed_alone() {
    let p = protocol(1);
    let candidates = || -> Vec<Candidate> {
        p.group
            .iter()
            .flat_map(|g| {
                g.images.iter().map(|id| Candidate {
                    image: id.clone(),
                    group: g.name.clone(),
                    a: shown(&format!("a-{id}")),
                    b: shown(&format!("b-{id}")),
                })
            })
            .collect()
    };
    let once = Assignment::draw(&p, "sha256:p", ["a".into(), "b".into()], candidates());
    let again = Assignment::draw(&p, "sha256:p", ["a".into(), "b".into()], candidates());
    assert_eq!(once, again);
    assert_eq!(once.pair.len(), 9);
    let order: Vec<&str> = once.pair.iter().map(|p| p.id.as_str()).collect();
    assert_ne!(
        order,
        ALL.to_vec(),
        "the order is drawn, not the protocol's"
    );
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(sorted, ALL.to_vec(), "every image once");
    assert!(
        once.pair.iter().any(|p| p.a_side == "left")
            && once.pair.iter().any(|p| p.a_side == "right")
    );
    for pair in &once.pair {
        let (a, b) = if pair.a_side == "left" {
            (&pair.left, &pair.right)
        } else {
            (&pair.right, &pair.left)
        };
        assert_eq!(a.cell, format!("a-{}", pair.id));
        assert_eq!(b.cell, format!("b-{}", pair.id));
    }
    let mut other = protocol(1);
    other.seed = 43;
    let differs = Assignment::draw(&other, "sha256:p", ["a".into(), "b".into()], candidates());
    assert_ne!(differs.pair, once.pair);
    // The generator itself: fixed values for a fixed seed.
    let mut draw = Draw::new(0);
    let first = draw.bits();
    assert_eq!(
        first, 0xE220_A839_7B1D_CDAF,
        "splitmix64's first output for seed 0"
    );
    let mut items = [1, 2, 3, 4, 5];
    Draw::new(7).shuffle(&mut items);
    let mut again = [1, 2, 3, 4, 5];
    Draw::new(7).shuffle(&mut again);
    assert_eq!(items, again);
}
