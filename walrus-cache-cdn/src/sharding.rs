use ahash::AHasher;
use std::hash::{Hash, Hasher};

pub fn pick_node<'a>(key: &str, nodes: &'a [String]) -> Option<&'a str> {
    if nodes.is_empty() {
        return None;
    }
    let mut best = None;
    let mut best_score = u64::MIN;
    for n in nodes {
        let mut h = AHasher::default();
        key.hash(&mut h);
        n.hash(&mut h);
        let score = h.finish();
        if score > best_score {
            best_score = score;
            best = Some(n.as_str());
        }
    }
    best
}
