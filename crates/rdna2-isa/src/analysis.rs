//! Register liveness and EXEC-mask regions over the control-flow graph.
//!
//! What each instruction reads and writes comes from AMD's specification,
//! including the operands it does not print: the SCC an `s_add_u32` sets, the
//! EXEC an `s_and_saveexec_b32` reads and writes, M0, and the VCC that VOPC
//! writes. Every vector instruction also reads EXEC, which picks its lanes.

use std::fmt::Write;

use crate::{Block, IMPLICIT, Instruction, Line, READ, WRITE, decode};

/// VCC, M0, EXEC and SCC as register numbers (the source operand encoding).
pub const VCC: u16 = 106;
pub const M0: u16 = 124;
pub const EXEC: u16 = 126;
pub const SCC: u16 = 253;

/// A set of registers, numbered like source operands: SGPRs and special
/// registers 0-255, VGPRs 256-511.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Regs([u64; 8]);

impl Regs {
    pub fn insert(&mut self, r: u16) {
        self.0[usize::from(r / 64) & 7] |= 1 << (r % 64);
    }

    pub fn contains(&self, r: u16) -> bool {
        self.0[usize::from(r / 64) & 7] >> (r % 64) & 1 == 1
    }

    pub fn union(self, other: Regs) -> Regs {
        Regs(std::array::from_fn(|i| self.0[i] | other.0[i]))
    }

    pub fn minus(self, other: Regs) -> Regs {
        Regs(std::array::from_fn(|i| self.0[i] & !other.0[i]))
    }

    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        (0..512).filter(|&r| self.contains(r))
    }

    /// Live VGPRs.
    pub fn vgprs(&self) -> u32 {
        self.0[4..].iter().map(|w| w.count_ones()).sum()
    }

    /// Live SGPRs `s0`-`s105`, the ones a kernel allocates.
    pub fn sgprs(&self) -> u32 {
        (0..106).filter(|&r| self.contains(r)).count() as u32
    }

    /// `s[0:3], v0, v[4:5], vcc_lo, exec_lo, scc`: runs of SGPRs and VGPRs as
    /// ranges, then the special registers by name.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        for (prefix, first, last) in [("s", 0, 105), ("v", 256, 511)] {
            let mut r = first;
            while r <= last {
                if !self.contains(r) {
                    r += 1;
                    continue;
                }
                let start = r;
                while r < last && self.contains(r + 1) {
                    r += 1;
                }
                let (a, b) = (start - first, r - first);
                parts.push(if a == b { format!("{prefix}{a}") } else { format!("{prefix}[{a}:{b}]") });
                r += 1;
            }
        }
        for (r, name) in [(VCC, "vcc_lo"), (VCC + 1, "vcc_hi"), (M0, "m0"), (EXEC, "exec_lo"), (EXEC + 1, "exec_hi"), (SCC, "scc")] {
            if self.contains(r) {
                parts.push(name.into());
            }
        }
        for r in (108..124).filter(|&r| self.contains(r)) {
            parts.push(format!("ttmp{}", r - 108));
        }
        parts.join(", ")
    }
}

/// Registers named in printed operand text: `s5`, `s[4:7]`, `-v1`, `|v2|`,
/// `[v4, v6]`, `vcc_lo`, `exec`, `m0`, `ttmp2`.
fn parse(text: &str, regs: &mut Regs) {
    let cleaned: String = text.chars().map(|c| if ",|()-".contains(c) { ' ' } else { c }).collect();
    for token in cleaned.split_whitespace() {
        let token = token.strip_prefix('[').unwrap_or(token);
        let token = if token.contains('[') { token } else { token.trim_end_matches(']') };
        match token {
            "vcc_lo" => regs.insert(VCC),
            "vcc_hi" => regs.insert(VCC + 1),
            "vcc" => [VCC, VCC + 1].map(|r| regs.insert(r)).into_iter().for_each(drop),
            "exec_lo" => regs.insert(EXEC),
            "exec_hi" => regs.insert(EXEC + 1),
            "exec" => [EXEC, EXEC + 1].map(|r| regs.insert(r)).into_iter().for_each(drop),
            "m0" => regs.insert(M0),
            "scc" => regs.insert(SCC),
            _ => {
                let (base, rest) = if let Some(rest) = token.strip_prefix("ttmp") {
                    (108, rest)
                } else if let Some(rest) = token.strip_prefix('s') {
                    (0, rest)
                } else if let Some(rest) = token.strip_prefix('v') {
                    (256, rest)
                } else {
                    continue;
                };
                let range = match rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                    Some(r) => r.split_once(':').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))),
                    None => rest.parse().ok().map(|n: u16| (n, n)),
                };
                if let Some((a, b)) = range {
                    for n in a..=b.min(255) {
                        regs.insert(base + n);
                    }
                }
            }
        }
    }
}

const VECTOR: [&str; 14] =
    ["vop1", "vop2", "vopc", "vop3", "vop3p", "vintrp", "ds", "mubuf", "mtbuf", "mimg", "exp", "flat", "flat_glbl", "flat_scratch"];

/// The registers an instruction reads and the ones it writes.
pub fn effects(inst: &Instruction) -> (Regs, Regs) {
    let (mut reads, mut writes) = (Regs::default(), Regs::default());
    let name = inst.name.unwrap_or("");
    // A lane mask is the low register alone in wave32.
    let mask = |regs: &mut Regs, low: u16| {
        regs.insert(low);
        if inst.wave64 {
            regs.insert(low + 1);
        }
    };
    for &operand in inst.operands {
        let (field, ty, _, flags) = operand;
        let mut regs = Regs::default();
        match ty {
            "SSRC_SPECIAL_SCC" => regs.insert(SCC),
            "SDST_EXEC" | "EXEC" => mask(&mut regs, EXEC),
            "SDST_M0" => regs.insert(M0),
            "VCC" if field.is_empty() => mask(&mut regs, VCC),
            // Memory and the program counter.
            _ if flags & IMPLICIT != 0 => {}
            _ => parse(&inst.operand(name, operand).unwrap_or_default(), &mut regs),
        }
        if flags & READ != 0 {
            reads = reads.union(regs);
        }
        if flags & WRITE != 0 {
            writes = writes.union(regs);
        }
    }
    if VECTOR.contains(&inst.format.name) {
        mask(&mut reads, EXEC);
    }
    (reads, writes)
}

/// Liveness and EXEC regions, one entry per line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    /// Registers live just before each line: read later before being written.
    pub live: Vec<Regs>,
    /// The writes of EXEC that reach each line, as line indices; `None` is
    /// the mask the wave started with.
    pub exec: Vec<Vec<Option<usize>>>,
}

/// Analyzes `lines` split into `blocks` (see [`crate::blocks`]). Blocks with
/// no predecessor (the first, and function entries) start with the dispatch
/// EXEC mask.
// shortcut: a VGPR write counts as overwriting every lane, so a value kept in
// lanes EXEC turned off is not tracked; tools like LLVM's do the same.
pub fn analyze(lines: &[Line], blocks: &[Block], wave64: bool) -> Analysis {
    let effects: Vec<(Regs, Regs)> = lines
        .iter()
        .map(|l| decode(&l.words).map_or_default(|i| effects(&Instruction { wave64, ..i })))
        .collect();
    let writes_exec = |i: usize| effects[i].1.contains(EXEC);

    // Liveness, backwards until nothing changes.
    let mut live_in = vec![Regs::default(); blocks.len()];
    let mut live = vec![Regs::default(); lines.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for (b, block) in blocks.iter().enumerate().rev() {
            let mut now = block.successors.iter().fold(Regs::default(), |r, &s| r.union(live_in[s]));
            for i in block.lines.clone().rev() {
                now = now.minus(effects[i].1).union(effects[i].0);
                live[i] = now;
            }
            if now != live_in[b] {
                live_in[b] = now;
                changed = true;
            }
        }
    }

    // Reaching writes of EXEC, forwards until nothing changes.
    let mut preds = vec![Vec::new(); blocks.len()];
    for (b, block) in blocks.iter().enumerate() {
        for &s in &block.successors {
            preds[s].push(b);
        }
    }
    let mut reach_out: Vec<Vec<Option<usize>>> = vec![Vec::new(); blocks.len()];
    let mut exec = vec![Vec::new(); lines.len()];
    changed = true;
    while changed {
        changed = false;
        for (b, block) in blocks.iter().enumerate() {
            let mut now: Vec<Option<usize>> = if preds[b].is_empty() {
                vec![None]
            } else {
                let mut all: Vec<_> = preds[b].iter().flat_map(|&p| reach_out[p].iter().copied()).collect();
                all.sort();
                all.dedup();
                all
            };
            for i in block.lines.clone() {
                exec[i] = now.clone();
                if writes_exec(i) {
                    now = vec![Some(i)];
                }
            }
            if now != reach_out[b] {
                reach_out[b] = now;
                changed = true;
            }
        }
    }
    Analysis { live, exec }
}

/// What a write of EXEC does, from its mnemonic: `if` (narrow and save),
/// `else`, `restore` (widen back), `cmpx` (narrow by a compare) or `set`.
pub fn exec_kind(text: &str) -> &'static str {
    let name = text.split(' ').next().unwrap_or("");
    if name.starts_with("v_cmpx") {
        "cmpx"
    } else if name.contains("and_saveexec") {
        "if"
    } else if name.contains("xor") || name.contains("andn2_saveexec") {
        "else"
    } else if name.starts_with("s_or") {
        "restore"
    } else if name.starts_with("s_andn2") {
        "break"
    } else {
        "set"
    }
}

/// A short label for the EXEC writes that reach a line: `entry`,
/// `0x1c if`, or several joined by ` | ` where paths merge.
pub fn exec_label(lines: &[Line], reaching: &[Option<usize>]) -> String {
    let mut out = String::new();
    for (n, r) in reaching.iter().enumerate() {
        if n > 0 {
            out += " | ";
        }
        match r {
            None => out += "entry",
            Some(i) => {
                let _ = write!(out, "{:#x} {}", lines[*i].offset, exec_kind(&lines[*i].text));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{blocks, disassemble};

    fn code(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn names(regs: Regs) -> String {
        regs.describe()
    }

    #[test]
    fn reads_and_writes_include_implicit_registers() {
        let inst = |words: &[u32]| effects(&decode(words).unwrap());
        // s_add_u32 s0, s1, s2 sets SCC.
        let (r, w) = inst(&[0x8000_0201]);
        assert_eq!((names(r), names(w)), ("s[1:2]".into(), "s0, scc".into()));
        // v_add_f32_e32 v0, v1, v2 reads EXEC for its lanes.
        let (r, w) = inst(&[0x0600_0501]);
        assert_eq!((names(r), names(w)), ("v[1:2], exec_lo".into(), "v0".into()));
        // Ranges: buffer_store_dword v0, v1, s[4:7], 0 offen offset:16 glc.
        let (r, _) = inst(&[0xE070_5010, 0x8001_0001]);
        assert_eq!(names(r), "s[4:7], v[0:1], exec_lo");
        // v_cmp_eq_f32_e32 vcc_lo, v0, v1 writes VCC.
        let (_, w) = inst(&[0x7C04_0300]);
        assert_eq!(names(w), "vcc_lo");
        // s_and_saveexec_b32 s0, vcc_lo: reads VCC and EXEC, writes s0, EXEC, SCC.
        let (r, w) = inst(&[0xBE80_3C6A]);
        assert_eq!((names(r), names(w)), ("vcc_lo, exec_lo".into(), "s0, exec_lo, scc".into()));
    }

    #[test]
    fn liveness_and_exec_regions_follow_the_branches() {
        // 0: v_cmp_eq_f32_e32 vcc_lo, v0, v1
        // 4: s_and_saveexec_b32 s0, vcc_lo     (if)
        // 8: s_cbranch_execz 1                 (to 0x10)
        // c: v_add_f32_e32 v2, v0, v1
        // 10: s_or_b32 exec_lo, exec_lo, s0    (restore)
        // 14: s_endpgm
        let lines = disassemble(&code(&[0x7C04_0300, 0xBE80_3C6A, 0xBF88_0001, 0x0604_0300, 0x887E_007E, 0xBF81_0000]), false);
        assert_eq!(lines[4].text, "s_or_b32 exec_lo, exec_lo, s0");
        let b = blocks(&lines, &[]);
        let a = analyze(&lines, &b, false);
        assert_eq!(names(a.live[0]), "v[0:1], exec_lo");
        // s0 holds the saved mask from the saveexec to the restore.
        assert_eq!(names(a.live[3]), "s0, v[0:1], exec_lo");
        assert_eq!(names(a.live[4]), "s0, exec_lo");
        assert_eq!(names(a.live[5]), "");
        assert_eq!(exec_label(&lines, &a.exec[0]), "entry");
        assert_eq!(exec_label(&lines, &a.exec[3]), "0x4 if");
        assert_eq!(exec_label(&lines, &a.exec[5]), "0x10 restore");
    }
}
