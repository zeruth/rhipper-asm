//! Two builds of a program matched function by function, for a binary diff: which function of
//! the new build each function of the old one became, and how much of its code changed.
//!
//! The matching runs in rounds, each on what the ones before matched. The build's symbols go
//! first, where both builds have them and the caller wants them. Then a function alone in each
//! build by what it is by itself (its strings, its imports, its recursion, that it never returns,
//! the vtable slots that hold it) matches, and one alone in each build by the shape of its code
//! (its instructions with the addresses taken out, as a function nothing changed has in both).
//! Then the call graph of the matched pairs: the functions the two sides of a pair call, in the
//! order of their first calls, line up between two callees already matched when the runs between
//! them are as long on both sides, and the one caller each side has left unmatched match. The
//! constants a function holds come last, when nothing else matches more. Every step takes only
//! what is unique on both sides and agrees in strings, imports and return: a function nothing
//! singles out stays unmatched rather than matched by a guess.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use ra_asm_arch::{Instr, Operand};
use serde::Serialize;

use crate::{Analysis, Function, Kind, MAX_ROUNDS, Origin};

/// What matched a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// The same symbol in both builds.
    Symbol,
    /// The same strings, imports and other features of the function by itself.
    Local,
    /// The same instructions, the addresses aside.
    Shape,
    /// The same place among the callees of a matched pair.
    Callee,
    /// The one unmatched caller of each side of a matched pair.
    Caller,
    /// The same features and the same constants.
    Constants,
}

/// A function of the old build and the one of the new build it became.
#[derive(Debug, Clone, Serialize)]
pub struct Pair {
    pub old: u64,
    pub new: u64,
    pub how: How,
    /// The round that matched it: 0 for the symbols.
    pub round: usize,
    /// How much of the code changed, in percent of the instructions: 0 for the same shape.
    pub change: u32,
}

/// The matching of two builds.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Matching {
    /// The matched functions, the most changed first.
    pub pairs: Vec<Pair>,
    /// The functions of the old build no function of the new one matched, by address.
    pub removed: Vec<u64>,
    /// The functions of the new build no function of the old one matched, by address.
    pub added: Vec<u64>,
}

impl Matching {
    /// The pairs whose code changed, the most changed first.
    pub fn changed(&self) -> impl Iterator<Item = &Pair> {
        self.pairs.iter().filter(|p| p.change > 0)
    }
}

/// Matches the functions of `old` to those of `new`; with `symbols`, the build's symbols match
/// first, and without, the matching is by structure alone. See the module's documentation.
pub fn match_functions(old: &Analysis<'_>, new: &Analysis<'_>, symbols: bool) -> Matching {
    let mut m = Matcher {
        old: Side::of(old, symbols),
        new: Side::of(new, symbols),
        old_to_new: HashMap::new(),
        new_to_old: HashMap::new(),
        pairs: Vec::new(),
        round: 0,
    };
    if symbols {
        m.unique(How::Symbol, |s, f| s.symbols.get(&f).cloned());
        m.propagate();
    }
    for round in 1..=MAX_ROUNDS {
        m.round = round;
        let before = m.pairs.len();
        m.unique(How::Local, |s, f| (!s.local[&f].is_empty()).then(|| s.local[&f].clone()));
        m.unique(How::Shape, |s, f| Some(s.shape[&f]));
        m.propagate();
        if m.pairs.len() == before {
            m.unique(How::Constants, |s, f| (!s.constants[&f].is_empty()).then(|| (s.local[&f].clone(), s.constants[&f].clone())));
            m.propagate();
            if m.pairs.len() == before {
                break;
            }
        }
    }
    let Matcher { old: a, new: b, old_to_new, new_to_old, mut pairs, .. } = m;
    for p in &mut pairs {
        p.change = if a.shape[&p.old] == b.shape[&p.new] { 0 } else { change(&a.tokens[&p.old], &b.tokens[&p.new]) };
    }
    pairs.sort_by(|x, y| y.change.cmp(&x.change).then(x.old.cmp(&y.old)));
    let removed = a.entries.iter().copied().filter(|f| !old_to_new.contains_key(f)).collect();
    let added = b.entries.iter().copied().filter(|f| !new_to_old.contains_key(f)).collect();
    Matching { pairs, removed, added }
}

/// What the matching knows of the functions of one build, by entry address.
#[derive(Default)]
struct Side {
    entries: Vec<u64>,
    symbols: HashMap<u64, String>,
    /// The features of the function by itself, sorted; no name among them without the symbols.
    local: HashMap<u64, Vec<String>>,
    /// What two functions must share to match: their strings, imports and return.
    agree: HashMap<u64, Vec<String>>,
    /// The immediates that are neither small nor an address of the image.
    constants: HashMap<u64, BTreeSet<i64>>,
    shape: HashMap<u64, u64>,
    /// The shape of each instruction, in address order.
    tokens: HashMap<u64, Vec<u64>>,
    /// The functions it calls or jumps to, in the order of their first calls.
    calls: HashMap<u64, Vec<u64>>,
    callers: HashMap<u64, Vec<u64>>,
}

impl Side {
    fn of(analysis: &Analysis<'_>, symbols: bool) -> Self {
        let mut side = Self::default();
        for f in analysis.functions.values() {
            let mut calls = Vec::new();
            let mut seen = HashSet::new();
            let mut tokens = Vec::new();
            let mut constants = BTreeSet::new();
            for i in f.blocks.iter().flat_map(|b| &b.instrs) {
                tokens.push(token(analysis, f, i));
                if let Some(t) = i.flow.direct_target()
                    && (i.flow.is_call() || !f.contains(t))
                    && analysis.functions.contains_key(&t)
                    && seen.insert(t)
                {
                    calls.push(t);
                }
                for o in &i.operands {
                    if let Operand::Imm { value, .. } = o
                        && value.unsigned_abs() > 0xff
                        && !analysis.image.is_mapped(*value as u64)
                    {
                        constants.insert(*value);
                    }
                }
            }
            let mut hasher = DefaultHasher::new();
            tokens.hash(&mut hasher);
            f.blocks.len().hash(&mut hasher);
            side.entries.push(f.entry);
            if let Some(symbol) = &f.symbol {
                side.symbols.insert(f.entry, symbol.clone());
            }
            side.local.insert(f.entry, local(analysis, f, symbols));
            side.agree.insert(f.entry, agree(f));
            side.constants.insert(f.entry, constants);
            side.shape.insert(f.entry, hasher.finish());
            side.tokens.insert(f.entry, tokens);
            for &callee in &calls {
                side.callers.entry(callee).or_default().push(f.entry);
            }
            side.calls.insert(f.entry, calls);
        }
        for &f in &side.entries {
            side.callers.entry(f).or_default();
        }
        side
    }
}

/// The features of `f` by itself, the names of the build left out without `symbols`.
fn local(analysis: &Analysis<'_>, f: &Function, symbols: bool) -> Vec<String> {
    let mut features: BTreeSet<String> = agree(f).into_iter().collect();
    for origin in &f.origins {
        match origin {
            Origin::Entry => features.insert("entry".to_owned()),
            Origin::TlsCallback => features.insert("tls callback".to_owned()),
            Origin::Export(name) if symbols => features.insert(format!("export {name}")),
            _ => false,
        };
    }
    if analysis.program_entry == Some(f.entry) {
        features.insert("program entry".to_owned());
    }
    if let Kind::Runtime(name) = &f.kind
        && symbols
    {
        features.insert(format!("runtime {name}"));
    }
    if f.calls.contains(&f.entry) || f.tail_calls.contains(&f.entry) {
        features.insert("recursive".to_owned());
    }
    features.extend(f.virtuals.iter().map(|v| format!("virtual {}+{}#{}", v.class, v.offset, v.slot)));
    features.into_iter().collect()
}

/// What two builds of a function share whatever the compiler did: the strings it addresses, the
/// imports it calls, the import it is the thunk of, and whether it returns.
fn agree(f: &Function) -> Vec<String> {
    let mut features = BTreeSet::new();
    features.extend(f.strings.iter().map(|(_, text)| format!("string {text:?}")));
    features.extend(f.import_calls.iter().map(|import| format!("import {import}")));
    if let Kind::ImportThunk(import) = &f.kind {
        features.insert(format!("thunk {import}"));
    }
    if f.noreturn {
        features.insert("noreturn".to_owned());
    }
    features.into_iter().collect()
}

/// The shape of an instruction: its mnemonic and operands, with an address of the image in place
/// of none and a branch inside the function by its offset from the entry.
fn token(analysis: &Analysis<'_>, f: &Function, i: &Instr) -> u64 {
    let mapped = |va: u64| analysis.image.is_mapped(va);
    let mut h = DefaultHasher::new();
    i.mnemonic.hash(&mut h);
    for o in &i.operands {
        match o {
            Operand::Reg(r) => format!("{r:?}").hash(&mut h),
            Operand::Imm { value, bits } => {
                bits.hash(&mut h);
                if mapped(*value as u64) { "address".hash(&mut h) } else { value.hash(&mut h) }
            }
            Operand::Mem(m) => {
                format!("{:?} {:?} {:?}", m.segment, m.base, m.index).hash(&mut h);
                (m.scale, m.bits).hash(&mut h);
                if m.absolute.is_some() || mapped(m.disp as u64) { "address".hash(&mut h) } else { m.disp.hash(&mut h) }
            }
            Operand::Addr(t) if f.contains(*t) => t.wrapping_sub(f.entry).hash(&mut h),
            Operand::Addr(_) => "target".hash(&mut h),
        }
    }
    h.finish()
}

/// How much of `old` is not in `new`, in percent: one less the share of the instructions the two
/// have in common, at least 1 when they differ only in order.
fn change(old: &[u64], new: &[u64]) -> u32 {
    let mut counts: HashMap<u64, usize> = HashMap::new();
    for t in old {
        *counts.entry(*t).or_default() += 1;
    }
    let mut common = 0;
    for t in new {
        if let Some(c) = counts.get_mut(t)
            && *c > 0
        {
            *c -= 1;
            common += 1;
        }
    }
    let total = old.len() + new.len();
    if total == 0 { 1 } else { (100 - 200 * common / total).max(1) as u32 }
}

struct Matcher {
    old: Side,
    new: Side,
    old_to_new: HashMap<u64, u64>,
    new_to_old: HashMap<u64, u64>,
    pairs: Vec<Pair>,
    round: usize,
}

impl Matcher {
    fn pair(&mut self, old: u64, new: u64, how: How) {
        if self.old_to_new.contains_key(&old) || self.new_to_old.contains_key(&new) {
            return;
        }
        self.old_to_new.insert(old, new);
        self.new_to_old.insert(new, old);
        self.pairs.push(Pair { old, new, how, round: self.round, change: 0 });
    }

    fn agree(&self, old: u64, new: u64) -> bool {
        self.old.agree.get(&old) == self.new.agree.get(&new)
    }

    /// Matches the unmatched functions whose `key` is had by one function of each build alone.
    fn unique<K: Hash + Eq>(&mut self, how: How, key: impl Fn(&Side, u64) -> Option<K>) {
        let mut groups: HashMap<K, (Vec<u64>, Vec<u64>)> = HashMap::new();
        for &f in &self.old.entries {
            if !self.old_to_new.contains_key(&f)
                && let Some(k) = key(&self.old, f)
            {
                groups.entry(k).or_default().0.push(f);
            }
        }
        for &f in &self.new.entries {
            if !self.new_to_old.contains_key(&f)
                && let Some(k) = key(&self.new, f)
            {
                groups.entry(k).or_default().1.push(f);
            }
        }
        let mut found: Vec<(u64, u64)> =
            groups.into_values().filter_map(|(old, new)| (old.len() == 1 && new.len() == 1).then(|| (old[0], new[0]))).collect();
        found.sort_unstable();
        for (old, new) in found {
            self.pair(old, new, how);
        }
    }

    /// Spreads the matches over the call graph until it matches nothing more.
    fn propagate(&mut self) {
        loop {
            let before = self.pairs.len();
            let mut n = 0;
            while n < self.pairs.len() {
                let (old, new) = (self.pairs[n].old, self.pairs[n].new);
                self.align_callees(old, new);
                self.lone_callers(old, new);
                n += 1;
            }
            if self.pairs.len() == before {
                break;
            }
        }
    }

    /// Lines up the callees of the pair `old`, `new`: the matched ones that both call, in the same
    /// order, part the two lists into runs, and two runs of unmatched callees as long as each
    /// other match place by place, when every place agrees.
    fn align_callees(&mut self, old: u64, new: u64) {
        let (ca, cb) = (&self.old.calls[&old], &self.new.calls[&new]);
        // A callee matched to one the other side does not call is out of the alignment.
        let sa: Vec<u64> = ca.iter().copied().filter(|x| self.old_to_new.get(x).is_none_or(|y| cb.contains(y))).collect();
        let sb: Vec<u64> = cb.iter().copied().filter(|y| self.new_to_old.get(y).is_none_or(|x| ca.contains(x))).collect();
        let same = |i: usize, j: usize| self.old_to_new.get(&sa[i]) == Some(&sb[j]);
        let (n, m) = (sa.len(), sb.len());
        let width = m + 1;
        let mut lcs = vec![0u32; (n + 1) * width];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i * width + j] = if same(i, j) { lcs[(i + 1) * width + j + 1] + 1 } else { lcs[(i + 1) * width + j].max(lcs[i * width + j + 1]) };
            }
        }
        let mut anchors = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if same(i, j) {
                anchors.push((i, j));
                i += 1;
                j += 1;
            } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
        anchors.push((n, m));
        let mut found = Vec::new();
        let (mut from_a, mut from_b) = (0, 0);
        for (i, j) in anchors {
            let ga: Vec<u64> = sa[from_a..i].iter().copied().filter(|x| !self.old_to_new.contains_key(x)).collect();
            let gb: Vec<u64> = sb[from_b..j].iter().copied().filter(|y| !self.new_to_old.contains_key(y)).collect();
            if !ga.is_empty() && ga.len() == gb.len() && ga.iter().zip(&gb).all(|(x, y)| self.agree(*x, *y)) {
                found.extend(ga.into_iter().zip(gb));
            }
            (from_a, from_b) = (i + 1, j + 1);
        }
        for (x, y) in found {
            self.pair(x, y, How::Callee);
        }
    }

    /// Matches the one unmatched caller of `old` to the one of `new`, when the two agree and what
    /// each calls that is matched, the other calls too.
    fn lone_callers(&mut self, old: u64, new: u64) {
        let ua: Vec<u64> = self.old.callers[&old].iter().copied().filter(|x| !self.old_to_new.contains_key(x)).collect();
        let ub: Vec<u64> = self.new.callers[&new].iter().copied().filter(|y| !self.new_to_old.contains_key(y)).collect();
        let ([x], [y]) = (ua.as_slice(), ub.as_slice()) else { return };
        let (x, y) = (*x, *y);
        let consistent = self.old.calls[&x].iter().all(|c| self.old_to_new.get(c).is_none_or(|d| self.new.calls[&y].contains(d)))
            && self.new.calls[&y].iter().all(|d| self.new_to_old.get(d).is_none_or(|c| self.old.calls[&x].contains(c)));
        if consistent && self.agree(x, y) {
            self.pair(x, y, How::Caller);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::change;

    #[test]
    fn the_change_is_the_share_of_the_instructions_not_in_common() {
        assert_eq!(change(&[1, 2, 3, 4], &[1, 2, 3, 5]), 25);
        assert_eq!(change(&[1, 2], &[2, 1]), 1);
        assert_eq!(change(&[1, 2], &[3, 4]), 100);
    }
}
