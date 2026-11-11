//! The orders and the dominators of a function's graph, which the raising passes and the
//! structuring share.

use std::collections::{HashMap, HashSet};

use crate::module::{BlockId, Function};

/// The blocks in reverse post-order from the entry: a block comes before every block it dominates,
/// and an unreachable block does not come at all.
pub fn reverse_post_order(f: &Function) -> Vec<BlockId> {
    let mut seen = HashSet::new();
    let mut post = Vec::new();
    let mut stack: Vec<(BlockId, Vec<BlockId>, usize)> = Vec::new();
    let entry = f.blocks[0].id;
    seen.insert(entry);
    stack.push((entry, f.block(entry).map(|b| b.terminator.successors()).unwrap_or_default(), 0));
    while let Some((b, ss, k)) = stack.last_mut() {
        if *k < ss.len() {
            let next = ss[*k];
            *k += 1;
            if seen.insert(next) {
                let next_succs = f.block(next).map(|b| b.terminator.successors()).unwrap_or_default();
                stack.push((next, next_succs, 0));
            }
        } else {
            post.push(*b);
            stack.pop();
        }
    }
    post.reverse();
    post
}

/// Immediate dominators by the iterative algorithm, over `order` (a reverse post-order) and the
/// predecessors of each block. The entry dominates itself.
pub fn dominators(order: &[BlockId], preds: &HashMap<BlockId, Vec<BlockId>>) -> HashMap<BlockId, BlockId> {
    let index: HashMap<BlockId, usize> = order.iter().enumerate().map(|(i, b)| (*b, i)).collect();
    let mut idom: HashMap<BlockId, BlockId> = HashMap::new();
    let Some(&entry) = order.first() else { return idom };
    idom.insert(entry, entry);
    let intersect = |idom: &HashMap<BlockId, BlockId>, mut a: BlockId, mut b: BlockId| -> BlockId {
        while a != b {
            while index[&a] > index[&b] {
                a = idom[&a];
            }
            while index[&b] > index[&a] {
                b = idom[&b];
            }
        }
        a
    };
    loop {
        let mut changed = false;
        for &b in order.iter().skip(1) {
            let mut new_idom: Option<BlockId> = None;
            for p in preds.get(&b).map_or(&Vec::new(), |p| p) {
                if !index.contains_key(p) || !idom.contains_key(p) {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => *p,
                    Some(n) => intersect(&idom, *p, n),
                });
            }
            if let Some(n) = new_idom
                && idom.get(&b) != Some(&n)
            {
                idom.insert(b, n);
                changed = true;
            }
        }
        if !changed {
            return idom;
        }
    }
}

/// Whether `a` dominates `b` (a block dominates itself).
pub fn dominates(idom: &HashMap<BlockId, BlockId>, a: BlockId, b: BlockId) -> bool {
    let mut x = b;
    loop {
        if x == a {
            return true;
        }
        let Some(&up) = idom.get(&x) else { return false };
        if up == x {
            return false;
        }
        x = up;
    }
}
