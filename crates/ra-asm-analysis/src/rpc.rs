//! The `PlayStation 2`'s remote procedure calls between the Emotion Engine and the I/O processor,
//! over the SIF. An IOP module registers a server under a 32-bit id with `sceSifRegisterRpc`,
//! naming the function that dispatches a request by its number; an EE program binds a client to
//! the id with `SifBindRpc` and calls through it with `SifCallRpc`, the function's number its
//! second argument. [`calls`] reads the EE side, [`servers`] the IOP side, and [`resolve`] joins
//! the two: a call of a constant number on a client bound to a constant id names the function the
//! module's dispatcher reaches for that number through its table.
//!
//! The arguments are the constants the registers hold at the call, found by a pass forward over
//! the function's blocks ([`constants_at_calls`]): `lui`, `addiu`, `ori` and the moves, as the
//! compiler builds an address or a number, through the saved registers across calls and loops.

use std::collections::{BTreeMap, HashMap};

use ra_asm_arch::{Flow, Instr, Operand, Target};

use crate::{Analysis, Function, Terminator};

/// The EE's call of a remote procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCall {
    /// The address of the `jal`.
    pub at: u64,
    /// The function that makes the call.
    pub caller: u64,
    /// The client the call goes through, when its address is a constant.
    pub client: Option<u64>,
    /// The id of the server every bind of the client names, when they agree on a constant.
    pub server: Option<u32>,
    /// The number of the function the call asks for, when a constant.
    pub function: Option<u32>,
}

/// An IOP module's server of remote procedures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcServer {
    /// The address of the call that registers it.
    pub at: u64,
    pub id: u32,
    /// The function each request goes to, with its number, its data and its size.
    pub dispatch: u64,
    /// The function each number reaches through the dispatcher's table, in number order; empty
    /// when the dispatcher reads no table.
    pub functions: Vec<(u32, u64)>,
}

/// The names the EE's library gives its calls: ps2sdk's `SifCallRpc` is a macro of `sceSifCallRpc`.
const CALL: &[&str] = &["SifCallRpc", "sceSifCallRpc"];
const BIND: &[&str] = &["SifBindRpc", "sceSifBindRpc"];
/// The import of `sifcmd` an IOP module registers a server with.
const REGISTER: &str = "sceSifRegisterRpc";

/// The registers of the o32 and n32 conventions: `a0`, `a1`, `a2`.
const A0: usize = 4;
const A1: usize = 5;
const A2: usize = 6;

/// Every `SifCallRpc` of an EE program, with the server its client is bound to.
pub fn calls(analysis: &Analysis<'_>) -> Vec<RpcCall> {
    let named = |names: &[&str]| -> Vec<u64> {
        analysis
            .functions
            .values()
            .filter(|f| names.contains(&f.name.as_str()) || f.symbol.as_deref().is_some_and(|s| names.contains(&s)))
            .map(|f| f.entry)
            .collect()
    };
    let (call, bind) = (named(CALL), named(BIND));
    if call.is_empty() {
        return Vec::new();
    }
    // The id each client is bound to; `None` once two binds disagree or one is not a constant.
    let mut bound: HashMap<u64, Option<u32>> = HashMap::new();
    let mut calls = Vec::new();
    for f in analysis.functions.values() {
        if !f.calls.iter().any(|c| call.contains(c) || bind.contains(c)) {
            continue;
        }
        for (at, target, regs) in constants_at_calls(analysis, f) {
            let client = regs[A0];
            if bind.contains(&target) {
                if let Some(client) = client {
                    let id = regs[A1].and_then(|id| u32::try_from(id).ok());
                    bound.entry(client).and_modify(|known| *known = known.filter(|&k| Some(k) == id)).or_insert(id);
                }
            } else if call.contains(&target) {
                calls.push(RpcCall { at, caller: f.entry, client, server: None, function: regs[A1].and_then(|n| u32::try_from(n).ok()) });
            }
        }
    }
    for c in &mut calls {
        c.server = c.client.and_then(|client| bound.get(&client).copied().flatten());
    }
    calls.sort_by_key(|c| c.at);
    calls
}

/// Every server an IOP module registers with a constant id, with the functions its dispatcher
/// reaches by number.
pub fn servers(analysis: &Analysis<'_>) -> Vec<RpcServer> {
    let image = analysis.image;
    let register: Vec<u64> = image.imports.iter().filter(|i| i.name.as_deref() == Some(REGISTER)).map(|i| i.iat_va).collect();
    if register.is_empty() {
        return Vec::new();
    }
    let mut servers = Vec::new();
    for f in analysis.functions.values().filter(|f| f.calls.iter().any(|c| register.contains(c))) {
        for (at, target, regs) in constants_at_calls(analysis, f) {
            let (true, Some(id), Some(dispatch)) = (register.contains(&target), regs[A1], regs[A2]) else { continue };
            let Ok(id) = u32::try_from(id) else { continue };
            let functions = analysis.function(dispatch).map(|f| dispatched(analysis, f)).unwrap_or_default();
            servers.push(RpcServer { at, id, dispatch, functions });
        }
    }
    servers.sort_by_key(|s| s.at);
    servers
}

/// The function a call reaches in the modules' servers: the one the server of its id dispatches
/// its number to, else the dispatcher itself when its table is not known; `None` when no server
/// has the id or the call's id or number is not a constant.
pub fn resolve(call: &RpcCall, servers: &[RpcServer]) -> Option<u64> {
    let server = servers.iter().find(|s| Some(s.id) == call.server)?;
    let number = call.function?;
    if server.functions.is_empty() {
        return Some(server.dispatch);
    }
    server.functions.iter().find(|&&(n, _)| n == number).map(|&(_, f)| f)
}

/// The functions a dispatcher's table holds by number: the cases of the switch it ends in, else
/// the table of function pointers it calls through.
fn dispatched(analysis: &Analysis<'_>, dispatch: &Function) -> Vec<(u32, u64)> {
    let mut functions: BTreeMap<u32, u64> = BTreeMap::new();
    for b in &dispatch.blocks {
        if let Terminator::Switch { cases, .. } = &b.terminator {
            for &(n, target) in cases {
                if let Ok(n) = u32::try_from(n) {
                    functions.entry(n).or_insert(target);
                }
            }
        }
    }
    if functions.is_empty() {
        return pointer_table(analysis, dispatch).unwrap_or_default();
    }
    functions.into_iter().collect()
}

/// The functions of a table of pointers a dispatcher indexes by the number: the one `sltiu` that
/// bounds the number gives the length, and the table is the data address the dispatcher builds
/// whose words up to it are every one a function's entry. `None` when no table reads so.
fn pointer_table(analysis: &Analysis<'_>, dispatch: &Function) -> Option<Vec<(u32, u64)>> {
    let image = analysis.image;
    let bounds: Vec<i64> = dispatch
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| i.mnemonic == "sltiu")
        .filter_map(|i| match i.operands.get(2) {
            Some(Operand::Imm { value, .. }) => Some(*value),
            _ => None,
        })
        .collect();
    let [bound] = bounds.as_slice() else { return None };
    let count = u32::try_from(*bound).ok().filter(|&n| n > 0 && n <= 1024)?;
    let mut addresses = Vec::new();
    for b in &dispatch.blocks {
        let mut regs: Regs = [None; 32];
        regs[0] = Some(0);
        regs[28] = image.global_pointer;
        for i in &b.instrs {
            step(i, &mut regs);
            if matches!(i.mnemonic.as_str(), "addiu" | "ori")
                && let Some(Operand::Reg(r)) = i.operands.first()
                && let Some(value) = usize::try_from(r.full).ok().filter(|&r| r < 32).and_then(|r| regs[r])
            {
                addresses.push(value);
            }
        }
    }
    addresses.into_iter().filter(|&a| !image.is_code(a)).find_map(|table| {
        (0..count)
            .map(|n| {
                let entry = u64::from(image.u32(table + 4 * u64::from(n)).ok()?);
                analysis.function(entry).map(|f| (n, f.entry))
            })
            .collect::<Option<Vec<_>>>()
    })
}

/// What each general register holds: a constant, or `None` when not one known.
type Regs = [Option<u64>; 32];

/// The registers a call leaves as they were: `s0`-`s7`, `gp`, `sp` and `fp`.
const SAVED: [usize; 11] = [16, 17, 18, 19, 20, 21, 22, 23, 28, 29, 30];

/// Every direct call of `f`, with its target and the constants the registers hold when it is made
/// (after its delay slot). Each block starts from what all the blocks before it agree on, the
/// entry from `$gp` alone, until no block learns more.
fn constants_at_calls(analysis: &Analysis<'_>, f: &Function) -> Vec<(u64, u64, Regs)> {
    let mut entry: Regs = [None; 32];
    entry[0] = Some(0);
    entry[28] = analysis.image.global_pointer;
    let predecessors: HashMap<u64, Vec<u64>> = f.blocks.iter().fold(HashMap::new(), |mut map, b| {
        for s in b.terminator.successors() {
            map.entry(s).or_default().push(b.start);
        }
        map
    });
    let mut out: HashMap<u64, Regs> = HashMap::new();
    let mut calls = Vec::new();
    // A round per loop the values travel around is enough; a constant settles in a few.
    for _ in 0..16 {
        let mut changed = false;
        calls.clear();
        for b in &f.blocks {
            let before = predecessors.get(&b.start).map_or(&[][..], Vec::as_slice);
            let mut ins: Vec<&Regs> = before.iter().filter_map(|p| out.get(p)).collect();
            if b.start == f.entry {
                ins.push(&entry);
            } else if ins.is_empty() && !before.is_empty() {
                // None of the blocks before it has run yet: it waits for them rather than
                // starting from nothing known.
                continue;
            }
            let mut regs = meet(&ins);
            walk(&b.instrs, &mut regs, &mut calls);
            if out.get(&b.start) != Some(&regs) {
                out.insert(b.start, regs);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    calls
}

/// The registers every one of `ins` agrees on; none known without any.
fn meet(ins: &[&Regs]) -> Regs {
    let Some((first, rest)) = ins.split_first() else {
        let mut regs = [None; 32];
        regs[0] = Some(0);
        return regs;
    };
    let mut regs = **first;
    for other in rest {
        for (r, o) in regs.iter_mut().zip(other.iter()) {
            if *r != *o {
                *r = None;
            }
        }
    }
    regs
}

/// Runs a block's instructions over `regs`, recording each direct call with the registers as the
/// callee sees them: the delay slot after the `jal` runs before it.
fn walk(instrs: &[Instr], regs: &mut Regs, calls: &mut Vec<(u64, u64, Regs)>) {
    let mut n = 0;
    while n < instrs.len() {
        let i = &instrs[n];
        if let Flow::Call(target) = i.flow {
            if let Some(slot) = instrs.get(n + 1) {
                step(slot, regs);
                n += 1;
            }
            if let Target::Direct(target) = target {
                calls.push((i.address, target, *regs));
            }
            for (r, value) in regs.iter_mut().enumerate() {
                if !SAVED.contains(&r) && r != 0 {
                    *value = None;
                }
            }
        } else {
            step(i, regs);
        }
        n += 1;
    }
}

/// One instruction's effect on the constants.
fn step(i: &Instr, regs: &mut Regs) {
    let reg = |o: Option<&Operand>| match o {
        Some(Operand::Reg(r)) => usize::try_from(r.full).ok().filter(|&r| r < 32),
        _ => None,
    };
    let imm = |o: Option<&Operand>| match o {
        Some(Operand::Imm { value, .. }) => Some(*value),
        _ => None,
    };
    let word = |v: u64| v & 0xffff_ffff;
    let (dst, a, b) = (reg(i.operands.first()), i.operands.get(1), i.operands.get(2));
    let value = match i.mnemonic.as_str() {
        "lui" => imm(a).map(|v| word((v as u64) << 16)),
        "addiu" | "addi" | "daddiu" => reg(a).and_then(|s| regs[s]).zip(imm(b)).map(|(s, v)| word(s.wrapping_add(v as u64))),
        "ori" => reg(a).and_then(|s| regs[s]).zip(imm(b)).map(|(s, v)| s | (v as u64 & 0xffff)),
        "move" => reg(a).and_then(|s| regs[s]),
        "or" | "addu" | "daddu" => match (reg(a), reg(b)) {
            (Some(s), Some(0)) | (Some(0), Some(s)) => regs[s],
            _ => None,
        },
        _ => None,
    };
    // Whatever else the instruction writes is no longer known.
    for (r, access) in &i.registers {
        if access.writes()
            && let Ok(r) = usize::try_from(r.full)
            && r < 32
        {
            regs[r] = None;
        }
    }
    if let Some(d) = dst
        && i.flow == Flow::Next
        && matches!(i.mnemonic.as_str(), "lui" | "addiu" | "addi" | "daddiu" | "ori" | "move" | "or" | "addu" | "daddu")
    {
        regs[d] = value;
    }
    regs[0] = Some(0);
}
