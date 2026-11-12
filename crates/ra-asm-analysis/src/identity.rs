//! The identity of a function across builds of a program: what the function is by itself (the
//! entry, an export, the string literals it addresses, the imports it calls, its recursion, that
//! it never returns, the vtable slots that hold it) and where it sits in the call graph (what its
//! callers and its callees are by themselves), never its address. A patch or another optimization
//! level moves every function; the identity is how what the person attached to a function of one
//! build finds the function in the next ([`find`]).
//!
//! The finding is exact or there is none: the build's symbols when both builds have them, the
//! one function that is by itself what the identity says, else the one whose identity, call graph
//! included, shares the most with it, by a clear margin over the others and over a third of the
//! two. Two functions of the new build that the identity matches alike (`op_add` and `op_sub`,
//! called through the same table) match neither.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{Analysis, Function, Kind, Origin};

/// What a function is, apart from its address: sets of features, each a text such as
/// `import WriteFile` or `string "fail\r\n"`, sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// The name the build's symbols (a PDB or a link map) give the function, when they do.
    pub symbol: Option<String>,
    /// The features of the function itself.
    pub local: Vec<String>,
    /// The features of the functions that call it or jump to it, together.
    pub callers: Vec<String>,
    /// The features of the functions it calls or jumps to, together.
    pub callees: Vec<String>,
}

impl Identity {
    /// Every feature of the identity, the call graph's marked by their side.
    fn features(&self) -> BTreeSet<String> {
        let callers = self.callers.iter().map(|f| format!("< {f}"));
        let callees = self.callees.iter().map(|f| format!("> {f}"));
        self.local.iter().cloned().chain(callers).chain(callees).collect()
    }
}

/// The identity of every function of `analysis`, by entry address.
pub fn identities(analysis: &Analysis<'_>) -> BTreeMap<u64, Identity> {
    let local: HashMap<u64, Vec<String>> = analysis.functions.values().map(|f| (f.entry, local(analysis, f))).collect();
    let mut callers: HashMap<u64, BTreeSet<&str>> = HashMap::new();
    let mut callees: HashMap<u64, BTreeSet<&str>> = HashMap::new();
    for f in analysis.functions.values() {
        for &callee in f.calls.iter().chain(&f.tail_calls) {
            if callee == f.entry {
                continue;
            }
            if let Some(features) = local.get(&callee) {
                callees.entry(f.entry).or_default().extend(features.iter().map(String::as_str));
            }
            callers.entry(callee).or_default().extend(local[&f.entry].iter().map(String::as_str));
        }
    }
    let collect = |set: Option<&BTreeSet<&str>>| set.map_or_else(Vec::new, |s| s.iter().map(|f| (*f).to_owned()).collect());
    analysis
        .functions
        .values()
        .map(|f| {
            let identity = Identity {
                symbol: f.symbol.clone(),
                local: local[&f.entry].clone(),
                callers: collect(callers.get(&f.entry)),
                callees: collect(callees.get(&f.entry)),
            };
            (f.entry, identity)
        })
        .collect()
}

/// The features of `f` itself, sorted.
fn local(analysis: &Analysis<'_>, f: &Function) -> Vec<String> {
    let mut features = BTreeSet::new();
    for origin in &f.origins {
        match origin {
            Origin::Entry => features.insert("entry".to_owned()),
            Origin::Export(name) => features.insert(format!("export {name}")),
            Origin::TlsCallback => features.insert("tls callback".to_owned()),
            _ => false,
        };
    }
    if analysis.program_entry == Some(f.entry) {
        features.insert("program entry".to_owned());
    }
    match &f.kind {
        Kind::ImportThunk(import) => features.insert(format!("thunk {import}")),
        Kind::Runtime(name) => features.insert(format!("runtime {name}")),
        Kind::Normal => false,
    };
    features.extend(f.strings.iter().map(|(_, text)| format!("string {text:?}")));
    features.extend(f.import_calls.iter().map(|import| format!("import {import}")));
    if f.calls.contains(&f.entry) || f.tail_calls.contains(&f.entry) {
        features.insert("recursive".to_owned());
    }
    if f.noreturn {
        features.insert("noreturn".to_owned());
    }
    features.extend(f.virtuals.iter().map(|v| format!("virtual {}+{}#{}", v.class, v.offset, v.slot)));
    features.into_iter().collect()
}

/// The function of the build `identities` are of that `wanted`, an identity from another build,
/// is: see the module's documentation. `None` when no function is, or more than one alike.
pub fn find(wanted: &Identity, identities: &BTreeMap<u64, Identity>) -> Option<u64> {
    Index::new(identities).find(wanted)
}

/// The identities of one build, ready for many [`find`]s.
pub struct Index<'a> {
    /// Each function's address, identity and features.
    entries: Vec<(u64, &'a Identity, BTreeSet<String>)>,
    by_local: HashMap<&'a [String], Vec<usize>>,
    by_symbol: HashMap<&'a str, Vec<usize>>,
}

impl<'a> Index<'a> {
    pub fn new(identities: &'a BTreeMap<u64, Identity>) -> Self {
        let entries: Vec<(u64, &Identity, BTreeSet<String>)> = identities.iter().map(|(a, i)| (*a, i, i.features())).collect();
        let mut by_local: HashMap<&[String], Vec<usize>> = HashMap::new();
        let mut by_symbol: HashMap<&str, Vec<usize>> = HashMap::new();
        for (n, &(_, identity, _)) in entries.iter().enumerate() {
            by_local.entry(identity.local.as_slice()).or_default().push(n);
            if let Some(symbol) = &identity.symbol {
                by_symbol.entry(symbol.as_str()).or_default().push(n);
            }
        }
        Self { entries, by_local, by_symbol }
    }

    /// See [`find`].
    pub fn find(&self, wanted: &Identity) -> Option<u64> {
        if let Some(symbol) = &wanted.symbol
            && let Some([one]) = self.by_symbol.get(symbol.as_str()).map(Vec::as_slice)
        {
            return Some(self.entries[*one].0);
        }
        let alike = self.by_local.get(wanted.local.as_slice()).map_or(&[][..], Vec::as_slice);
        if !wanted.local.is_empty()
            && let [one] = alike
        {
            return Some(self.entries[*one].0);
        }
        // The function changed itself (a string folded in, an import call inlined): its call graph decides.
        let all: Vec<usize>;
        let pool = if alike.is_empty() {
            all = (0..self.entries.len()).collect();
            &all[..]
        } else {
            alike
        };
        let features = wanted.features();
        let mut best: Option<(u64, usize, usize)> = None;
        let mut tie = false;
        for &n in pool {
            let (address, _, theirs) = &self.entries[n];
            let shared = features.intersection(theirs).count();
            let union = features.len() + theirs.len() - shared;
            if shared == 0 || 3 * shared < union {
                continue;
            }
            match best {
                Some((_, s, u)) if shared * u == s * union => tie = true,
                Some((_, s, u)) if shared * u < s * union => {}
                _ => {
                    best = Some((*address, shared, union));
                    tie = false;
                }
            }
        }
        if tie { None } else { best.map(|(address, _, _)| address) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(local: &[&str], callers: &[&str]) -> Identity {
        Identity {
            local: local.iter().map(|s| (*s).to_owned()).collect(),
            callers: callers.iter().map(|s| (*s).to_owned()).collect(),
            ..Identity::default()
        }
    }

    #[test]
    fn a_function_alone_in_what_it_is_is_found_wherever_it_moved() {
        let new = BTreeMap::from([(0x2000, identity(&["recursive"], &["entry"])), (0x3000, identity(&[], &["entry"]))]);
        assert_eq!(find(&identity(&["recursive"], &[]), &new), Some(0x2000));
    }

    #[test]
    fn the_call_graph_tells_apart_what_is_alike_by_itself_and_a_tie_finds_nothing() {
        let callers = ["entry", "string \"A\"", "string \"B\""];
        let new = BTreeMap::from([
            (0x2000, identity(&[], &["entry", "string \"A\"", "string \"B\"", "string \"C\""])),
            (0x3000, identity(&[], &["entry"])),
            (0x4000, identity(&[], &["entry"])),
        ]);
        assert_eq!(find(&identity(&[], &callers), &new), Some(0x2000));
        assert_eq!(find(&identity(&[], &["entry"]), &new), None);
        assert_eq!(find(&identity(&[], &[]), &new), None);
    }

    #[test]
    fn the_symbols_of_both_builds_decide_first() {
        let mut named = identity(&[], &[]);
        named.symbol = Some("fib".into());
        let new = BTreeMap::from([(0x2000, identity(&["recursive"], &[])), (0x3000, named.clone())]);
        assert_eq!(find(&named, &new), Some(0x3000));
    }
}
