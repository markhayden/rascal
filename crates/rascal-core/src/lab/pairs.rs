//! Blind pairs and their decode (SPEC.md "The lab"). An assignment pairs two arms' cells per
//! protocol image, with the page order and the side each arm takes drawn from the protocol's
//! seed; the page never names the arms. The picks a judge makes name a side per pair, and
//! `decode` turns them into wins and losses per group under the protocol's rules.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::lab::protocol::Protocol;

/// `pairs/assignment.json`: what `decode` needs and the page must not show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub assignment_version: u32,
    pub lab: String,
    pub protocol_digest: String,
    /// The two arms compared, `[a, b]`: wins are a's.
    pub arms: [String; 2],
    pub seed: u64,
    /// In page order.
    pub pair: Vec<Pair>,
}

/// One pair: an image, the two cells shown, and which side arm `a` took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    /// The pair id: the image id.
    pub id: String,
    pub group: String,
    pub left: Shown,
    pub right: Shown,
    /// `left` or `right`: the side arm `a` is on.
    pub a_side: String,
}

/// A cell's attempt as shown on one side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shown {
    pub cell: String,
    pub attempt: u32,
}

/// `picks.json`, as the page writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Picks {
    pub picks_version: u32,
    /// `sha256:<hex>` of the assignment file the picks were made on.
    pub assignment_digest: String,
    pub pick: Vec<Pick>,
}

/// One pick: `left`, `right`, `same` or `neither` for a pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pick {
    pub pair: String,
    pub pick: String,
}

/// The four picks.
pub const PICKS: [&str; 4] = ["left", "right", "same", "neither"];

/// A deterministic draw from a seed: splitmix64.
pub struct Draw(u64);

impl Draw {
    pub fn new(seed: u64) -> Draw {
        Draw(seed)
    }

    /// The next 64 bits.
    pub fn bits(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// An index below `n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.bits() % n as u64) as usize
    }

    /// Fisher–Yates over `items`.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

/// A cell of one arm on one image, with the attempt in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub image: String,
    pub group: String,
    pub a: Shown,
    pub b: Shown,
}

impl Assignment {
    /// The assignment for `candidates` (one per protocol image, in protocol order) under the
    /// protocol: the order shuffled and each pair's side drawn from the protocol's seed.
    pub fn draw(
        protocol: &Protocol,
        protocol_digest: &str,
        arms: [String; 2],
        candidates: Vec<Candidate>,
    ) -> Assignment {
        let mut draw = Draw::new(protocol.seed);
        let mut order: Vec<usize> = (0..candidates.len()).collect();
        draw.shuffle(&mut order);
        let pair = order
            .into_iter()
            .map(|i| {
                let c = &candidates[i];
                let a_left = draw.bits() & 1 == 0;
                let (left, right) = if a_left {
                    (c.a.clone(), c.b.clone())
                } else {
                    (c.b.clone(), c.a.clone())
                };
                Pair {
                    id: c.image.clone(),
                    group: c.group.clone(),
                    left,
                    right,
                    a_side: if a_left { "left" } else { "right" }.to_string(),
                }
            })
            .collect();
        Assignment {
            assignment_version: 1,
            lab: protocol.name.clone(),
            protocol_digest: protocol_digest.to_string(),
            arms,
            seed: protocol.seed,
            pair,
        }
    }
}

/// One group's decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupResult {
    pub group: String,
    pub pairs: u32,
    pub decided: u32,
    pub wins: u32,
    pub losses: u32,
    pub same: u32,
    pub neither: u32,
    /// `preferred`, `not_worse`, `worse` or `inconclusive`.
    pub outcome: &'static str,
}

/// The decode of a picks file against its assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decode {
    pub arms: [String; 2],
    pub groups: Vec<GroupResult>,
}

/// Decodes `picks` made on `assignment` (whose file digest is `assignment_digest`) under the
/// protocol: a digest that does not match, a pair missing, picked twice or not in the
/// assignment, or a pick that is not one of the four, is refused naming it.
pub fn decode(
    protocol: &Protocol,
    assignment: &Assignment,
    assignment_digest: &str,
    picks: &Picks,
) -> std::result::Result<Decode, String> {
    if picks.picks_version != 1 {
        return Err(format!("picks_version {} is not 1", picks.picks_version));
    }
    if picks.assignment_digest != assignment_digest {
        return Err(format!(
            "the picks were made on assignment {} but this one is {assignment_digest}; stale or foreign picks",
            picks.assignment_digest
        ));
    }
    let pairs: BTreeMap<&str, &Pair> = assignment.pair.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut seen = BTreeSet::new();
    let mut by_pair: BTreeMap<&str, &str> = BTreeMap::new();
    for pick in &picks.pick {
        if !pairs.contains_key(pick.pair.as_str()) {
            return Err(format!(
                "pair {} is not in the assignment (foreign pick)",
                pick.pair
            ));
        }
        if !seen.insert(pick.pair.as_str()) {
            return Err(format!("pair {} is picked twice", pick.pair));
        }
        if !PICKS.contains(&pick.pick.as_str()) {
            return Err(format!(
                "pair {}: pick {:?} is not one of left, right, same, neither",
                pick.pair, pick.pick
            ));
        }
        by_pair.insert(&pick.pair, &pick.pick);
    }
    for id in pairs.keys() {
        if !seen.contains(id) {
            return Err(format!("pair {id} has no pick (missing)"));
        }
    }
    let mut groups = Vec::new();
    // A page drawn for one group holds that group's pairs only; the others are not reported.
    for group in protocol
        .group
        .iter()
        .filter(|g| assignment.pair.iter().any(|p| p.group == g.name))
    {
        let mut result = GroupResult {
            group: group.name.clone(),
            pairs: 0,
            decided: 0,
            wins: 0,
            losses: 0,
            same: 0,
            neither: 0,
            outcome: "inconclusive",
        };
        for pair in assignment.pair.iter().filter(|p| p.group == group.name) {
            result.pairs += 1;
            match by_pair[pair.id.as_str()] {
                "same" => result.same += 1,
                "neither" => result.neither += 1,
                side => {
                    result.decided += 1;
                    if side == pair.a_side {
                        result.wins += 1;
                    } else {
                        result.losses += 1;
                    }
                }
            }
        }
        let [pn, pd] = protocol.rules.preferred;
        let [nn, nd] = protocol.rules.not_worse;
        result.outcome = if result.decided < protocol.min_decided {
            "inconclusive"
        } else if u64::from(result.wins) * u64::from(pd)
            >= u64::from(pn) * u64::from(result.decided)
        {
            "preferred"
        } else if u64::from(result.losses) * u64::from(nd)
            < u64::from(nn) * u64::from(result.decided)
        {
            "not_worse"
        } else {
            "worse"
        };
        groups.push(result);
    }
    Ok(Decode {
        arms: assignment.arms.clone(),
        groups,
    })
}
