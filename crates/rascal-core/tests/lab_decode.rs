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
    // Sky: 4 wins, 2 losses of 6 decided (2/3): preferred. Structured: 1 win, 1 loss, 1 same:
    // 2 decided, under min_decided 3: inconclusive.
    let d = decode(
        &p,
        &a,
        digest,
        &full(
            ["left"; 6]
                .map(|_| "left")
                .into_iter()
                .enumerate()
                .map(|(i, s)| if i < 4 { s } else { "right" })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            ["left", "right", "same"],
        ),
    )
    .unwrap();
    assert_eq!(d.arms, ["a".to_string(), "b".to_string()]);
    let sky = &d.groups[0];
    assert_eq!(
        (
            sky.pairs,
            sky.decided,
            sky.wins,
            sky.losses,
            sky.same,
            sky.neither
        ),
        (6, 6, 4, 2, 0, 0)
    );
    assert_eq!(sky.outcome, "preferred");
    let structured = &d.groups[1];
    assert_eq!(
        (
            structured.decided,
            structured.wins,
            structured.losses,
            structured.same
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(structured.outcome, "inconclusive");
    // Sky: 3 wins, 3 losses: not preferred, losses 1/2 not below 1/3: worse.
    let d = decode(
        &p,
        &a,
        digest,
        &full(
            ["left", "left", "left", "right", "right", "right"],
            ["left", "left", "left"],
        ),
    )
    .unwrap();
    assert_eq!(d.groups[0].outcome, "worse");
    assert_eq!(d.groups[1].outcome, "preferred");
    // Sky: 3 wins, 1 loss, 2 same: 4 decided, wins 3/4 ≥ 2/3: preferred; same ignored.
    let d = decode(
        &p,
        &a,
        digest,
        &full(
            ["left", "left", "left", "right", "same", "same"],
            ["neither", "neither", "neither"],
        ),
    )
    .unwrap();
    assert_eq!((d.groups[0].decided, d.groups[0].outcome), (4, "preferred"));
    assert_eq!(
        (
            d.groups[1].decided,
            d.groups[1].neither,
            d.groups[1].outcome
        ),
        (0, 3, "inconclusive")
    );
    // Sky: 2 wins, 1 loss, 3 neither: 3 decided, wins 2/3: preferred (the bound is inclusive).
    let d = decode(
        &p,
        &a,
        digest,
        &full(
            ["left", "left", "right", "neither", "neither", "neither"],
            ["left", "left", "right"],
        ),
    )
    .unwrap();
    assert_eq!(d.groups[0].outcome, "preferred");
    // Sky: 3 wins, 1 loss, of 6 decided? 3 wins 2 losses 1 neither: 5 decided, wins 3/5 < 2/3,
    // losses 2/5 ≥ 1/3: worse. 3 wins 1 loss 2 neither: 4 decided, wins 3/4: preferred.
    let d = decode(
        &p,
        &a,
        digest,
        &full(
            ["left", "left", "left", "right", "right", "neither"],
            ["left", "left", "right"],
        ),
    )
    .unwrap();
    assert_eq!(d.groups[0].outcome, "worse");
    // Not worse: 3 wins, 1 loss, 2 ... no: wins 3/6 with 1 loss and 2 same: decided 4, wins
    // 3/4 preferred. A not_worse case: 2 wins, 1 loss, 1 same, 2 neither on a 4-decided?
    // decided 3: wins 2/3 preferred. Use 6 decided, 3 wins, 1 loss impossible; take 5
    // decided: 3 wins, 1 loss ... = 4. So: 6 decided with 3 wins and 1 loss cannot be; a
    // not_worse outcome needs losses < 1/3 and wins < 2/3: 4 decided, 2 wins, 1 loss, 1 ...
    // no, decided counts only wins and losses. 7 decided is impossible here; with min 3 and
    // sky's 6 pairs: 6 decided, 3 wins, 3 losses is worse. not_worse needs e.g. 4 wins of 7.
    let wide = Protocol::parse(
        "version = 1\nname = \"t\"\nseed = 1\narms = [\"a\", \"b\"]\nmin_decided = 3\n[[group]]\nname = \"g\"\nimages = [\"i1\", \"i2\", \"i3\", \"i4\", \"i5\", \"i6\", \"i7\"]\n",
        "t",
    )
    .unwrap();
    let wa = assignment(&wide);
    let d = decode(
        &wide,
        &wa,
        digest,
        &picks(&[
            ("i1", "left"),
            ("i2", "left"),
            ("i3", "left"),
            ("i4", "left"),
            ("i5", "right"),
            ("i6", "right"),
            ("i7", "same"),
        ]),
    )
    .unwrap();
    // 6 decided, 4 wins (2/3): preferred.
    assert_eq!(d.groups[0].outcome, "preferred");
    let d = decode(
        &wide,
        &wa,
        digest,
        &picks(&[
            ("i1", "left"),
            ("i2", "left"),
            ("i3", "left"),
            ("i4", "left"),
            ("i5", "right"),
            ("i6", "right"),
            ("i7", "right"),
        ]),
    )
    .unwrap();
    // 7 decided, 4 wins (4/7 < 2/3), 3 losses (3/7 ≥ 1/3): worse.
    assert_eq!(d.groups[0].outcome, "worse");
    let d = decode(
        &wide,
        &wa,
        digest,
        &picks(&[
            ("i1", "left"),
            ("i2", "left"),
            ("i3", "left"),
            ("i4", "left"),
            ("i5", "right"),
            ("i6", "right"),
            ("i7", "left"),
        ]),
    )
    .unwrap();
    // 7 decided, 5 wins (5/7 ≥ 2/3): preferred.
    assert_eq!(d.groups[0].outcome, "preferred");
    let d = decode(
        &wide,
        &wa,
        digest,
        &picks(&[
            ("i1", "left"),
            ("i2", "left"),
            ("i3", "left"),
            ("i4", "right"),
            ("i5", "right"),
            ("i6", "same"),
            ("i7", "same"),
        ]),
    )
    .unwrap();
    // 5 decided, 3 wins (3/5 < 2/3), 2 losses (2/5 ≥ 1/3): worse.
    assert_eq!(d.groups[0].outcome, "worse");
    let d = decode(
        &wide,
        &wa,
        digest,
        &picks(&[
            ("i1", "left"),
            ("i2", "left"),
            ("i3", "left"),
            ("i4", "right"),
            ("i5", "same"),
            ("i6", "same"),
            ("i7", "same"),
        ]),
    )
    .unwrap();
    // 4 decided, 3 wins (3/4): preferred.
    assert_eq!(d.groups[0].outcome, "preferred");
    // not_worse: 7 decided, 4 wins is worse (3 losses); 6 decided 3 wins 3 losses worse; the
    // not_worse band is wins/decided < 2/3 and losses/decided < 1/3, so decided ≥ 4 with a
    // same: 5 decided, 3 wins (3/5), 2 losses (2/5) no. 8 would be 5 wins (5/8 < 2/3), 3
    // losses (3/8 ≥ 1/3) no. With the default rules not_worse needs wins + losses with
    // losses < decided/3 and wins < 2·decided/3, impossible since wins + losses = decided.
    // So not_worse is reachable only under a protocol whose fractions leave a gap.
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
    // 7 decided, 5 wins (5/7 < 3/4), 2 losses (2/7 < 1/3): not_worse.
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
