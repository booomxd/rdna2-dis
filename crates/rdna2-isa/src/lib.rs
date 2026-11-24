//! Decoding of AMD RDNA2 (gfx10.3) machine code. Formats, fields, opcodes
//! and operand lists are generated from AMD's machine-readable ISA
//! specification (`cargo run -p xtask`); the text follows `llvm-objdump`.

use std::fmt;

mod generated;

/// An explicit operand: field (empty for a fixed VCC/EXEC), type, bits.
type Operand = (&'static str, &'static str, u32);

/// One instruction format: its fixed bits, fields and opcodes.
#[derive(Debug)]
pub struct Spec {
    pub name: &'static str,
    /// Bits without trailing literal or address dwords.
    pub bits: u32,
    mask: u64,
    ids: &'static [u64],
    /// Field name and bit ranges (offset, count), least significant first.
    fields: &'static [(&'static str, &'static [(u8, u8)])],
    /// Opcode, mnemonic, whether a literal dword always follows, operands.
    opcodes: &'static [(u32, &'static str, bool, &'static [Operand])],
}

impl Spec {
    /// The value of field `name` in `raw`, if this format has the field.
    pub fn field(&self, raw: u64, name: &str) -> Option<u64> {
        let (_, ranges) = self.fields.iter().find(|(n, _)| *n == name)?;
        let mut value = 0;
        let mut shift = 0;
        for &(offset, count) in *ranges {
            value |= (raw >> offset & ((1 << count) - 1)) << shift;
            shift += count;
        }
        Some(value)
    }

    fn defines(&self, name: &str) -> bool {
        self.opcodes.iter().any(|o| o.1 == name)
    }
}

fn format(name: &str) -> &'static Spec {
    generated::ENCODINGS.iter().find(|s| s.name == name).unwrap()
}

/// Source operand values that mean "a constant dword follows".
const LITERAL: u64 = 255;
/// VOP1/VOP2/VOPC SRC0 values selecting a DPP8, DPP8 FI, SDWA or DPP16
/// dword after the instruction.
const EXTRA_DWORD: [u64; 4] = [0xE9, 0xEA, 0xF9, 0xFA];
const DPP8: u64 = 0xE9;
const DPP8_FI: u64 = 0xEA;
const SDWA: u64 = 0xF9;
const NULL: u64 = 125;

#[derive(Debug)]
pub struct Instruction {
    pub format: &'static Spec,
    pub opcode: u32,
    /// `None` for an opcode the format does not define.
    pub name: Option<&'static str>,
    /// Dwords including literal, DPP/SDWA and address dwords.
    pub dwords: usize,
    /// The instruction's dwords, as many of them as were given.
    pub words: Vec<u32>,
    operands: &'static [Operand],
}

/// Decodes the instruction at the start of `words`.
pub fn decode(words: &[u32]) -> Option<Instruction> {
    let raw = u64::from(*words.first()?) | u64::from(words.get(1).copied().unwrap_or(0)) << 32;
    // The masks overlap (every SOP1 word has SOP2's prefix); the longest wins.
    let format = generated::ENCODINGS
        .iter()
        .filter(|s| s.ids.contains(&(raw & s.mask)))
        .max_by_key(|s| s.mask.count_ones())?;
    let opcode = format.field(raw, "OP").unwrap_or(0) as u32;
    let entry = format.opcodes.iter().find(|o| o.0 == opcode);
    let mut dwords = format.bits as usize / 32;
    let sources = ["SSRC0", "SSRC1", "SRC0", "SRC1", "SRC2"];
    if entry.is_some_and(|o| o.2) || sources.iter().any(|s| format.field(raw, s) == Some(LITERAL)) {
        dwords += 1;
    }
    if matches!(format.name, "vop1" | "vop2" | "vopc")
        && format.field(raw, "SRC0").is_some_and(|s| EXTRA_DWORD.contains(&s))
    {
        dwords += 1;
    }
    dwords += format.field(raw, "NSA").unwrap_or(0) as usize;
    Some(Instruction {
        format,
        opcode,
        name: entry.map(|o| o.1),
        dwords,
        words: words[..dwords.min(words.len())].to_vec(),
        operands: entry.map_or(&[], |o| o.3),
    })
}

/// A raw code blob, instruction by instruction: byte offset, assembly in
/// `llvm-objdump`'s spelling, and the instruction's dwords.
pub fn disassemble(bytes: &[u8]) -> Vec<(usize, String, Vec<u32>)> {
    let words: Vec<u32> = bytes.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
    let mut lines = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let decoded = decode(&words[i..]);
        let n = decoded.as_ref().map_or(1, |d| d.dwords.min(words.len() - i));
        let text = decoded.map_or("<unknown format>".into(), |d| d.to_string());
        lines.push((i * 4, text, words[i..i + n].to_vec()));
        i += n;
    }
    lines
}

/// `s5`, `s[4:7]`, `vcc`, `v[0:3]`, `ttmp2`, or a special name (`m0`, `0.5`).
fn register(value: u64, count: u64) -> String {
    let range = |prefix: &str, n: u64| match count {
        1 => format!("{prefix}{n}"),
        _ => format!("{prefix}[{n}:{}]", n + count - 1),
    };
    match value {
        0..=105 => range("s", value),
        106 if count == 2 => "vcc".into(),
        126 if count == 2 => "exec".into(),
        108..=123 => range("ttmp", value - 108),
        256..=511 => range("v", value - 256),
        _ => name_in(generated::SRC_NAMES, value),
    }
}

fn name_in(table: &[(u32, &str)], value: u64) -> String {
    table.iter().find(|n| u64::from(n.0) == value).map_or(value.to_string(), |n| n.1.to_string())
}

/// `[a,b,c]` from the low `n` bits of `bits`.
fn bit_list(bits: u64, n: usize) -> String {
    let list: Vec<String> = (0..n).map(|i| (bits >> i & 1).to_string()).collect();
    format!("[{}]", list.join(","))
}

/// gfx10 layout: vmcnt in bits 3:0 and 15:14, expcnt 6:4, lgkmcnt 13:8.
fn waitcnt(v: u64) -> String {
    let counters = [
        ("vmcnt", (v & 0xF) | (v >> 14 & 3) << 4, 63),
        ("expcnt", v >> 4 & 7, 7),
        ("lgkmcnt", v >> 8 & 0x3F, 63),
    ];
    let waited: Vec<String> = counters.iter().filter(|c| c.1 != c.2).map(|c| format!("{}({})", c.0, c.1)).collect();
    if waited.is_empty() {
        counters.map(|c| format!("{}({})", c.0, c.1)).join(" ")
    } else {
        waited.join(" ")
    }
}

fn hwreg(v: u64) -> String {
    let (id, offset, size) = (v & 0x3F, v >> 6 & 0x1F, (v >> 11 & 0x1F) + 1);
    // LLVM shortens a few of AMD's names.
    let name = name_in(generated::HWREG_NAMES, id).replace("SHADER_FLAT_SCRATCH", "FLAT_SCR").replace("SHADER_T", "T");
    if offset == 0 && size == 32 {
        format!("hwreg({name})")
    } else {
        format!("hwreg({name}, {offset}, {size})")
    }
}

/// gfx10 layout: message in bits 3:0, operation 6:4, GS stream 9:8.
fn sendmsg(v: u64) -> String {
    let (msg, op, stream) = (v & 0xF, v >> 4 & 7, v >> 8 & 3);
    let named = |prefix: &str, n: u64| generated::SENDMSG_NAMES.iter().find(|e| u64::from(e.0) == n && e.1.starts_with(prefix)).map(|e| e.1);
    let raw = format!("sendmsg({msg}, {op}, {stream})");
    if v >> 10 != 0 {
        return v.to_string();
    }
    let Some(name) = named("MSG_", msg) else { return raw };
    match name {
        // GS_OP_NOP is only valid for GS_DONE; a stream only goes with an emit or cut.
        "MSG_GS" | "MSG_GS_DONE" => match named("GS_OP_", op) {
            Some(op_name) if op != 0 => format!("sendmsg({name}, {op_name}, {stream})"),
            Some(op_name) if name == "MSG_GS_DONE" && stream == 0 => format!("sendmsg({name}, {op_name})"),
            _ => raw,
        },
        // shortcut: MSG_SYSMSG's operations are not named; printed raw.
        "MSG_SYSMSG" => raw,
        _ if op == 0 && stream == 0 => format!("sendmsg({name})"),
        _ => raw,
    }
}

/// Address VGPRs of an image instruction, from its name and dimension.
// shortcut: A16/G16 packing is ignored; counts assume 32-bit addresses.
fn image_addresses(name: &str, dim: usize) -> u64 {
    if name.starts_with("image_get_resinfo") {
        return 1;
    }
    let coords = [1, 2, 3, 3, 2, 3, 3, 4][dim];
    let grads = [2, 4, 6, 4, 2, 4, 4, 4][dim];
    coords
        + name
            .split('_')
            .skip(2)
            .map(|t| match t {
                "c" | "b" | "l" | "o" | "cl" | "mip" => 1,
                "d" | "cd" => grads,
                _ => 0,
            })
            .sum::<u64>()
}

impl Instruction {
    fn word(&self, i: usize) -> u64 {
        u64::from(self.words.get(i).copied().unwrap_or(0))
    }

    fn field(&self, name: &str) -> u64 {
        self.format.field(self.word(0) | self.word(1) << 32, name).unwrap_or(0)
    }

    fn literal(&self) -> u64 {
        self.word(self.format.bits as usize / 32)
    }

    /// SRC0 of a VOP1/VOP2/VOPC with a DPP or SDWA dword after it.
    fn extra(&self) -> Option<u64> {
        let src0 = self.field("SRC0");
        (matches!(self.format.name, "vop1" | "vop2" | "vopc") && EXTRA_DWORD.contains(&src0)).then_some(src0)
    }

    /// `_e32`/`_e64` where both forms exist, `_dpp`/`_sdwa`, as LLVM spells it.
    fn suffix(&self, name: &str) -> &'static str {
        match (self.format.name, self.extra()) {
            (_, Some(SDWA)) => "_sdwa",
            (_, Some(_)) => "_dpp",
            ("vop1" | "vop2" | "vopc" | "vintrp", _) if format("vop3").defines(name) => "_e32",
            ("vop3", _) if ["vop1", "vop2", "vopc", "vintrp"].iter().any(|f| format(f).defines(name)) => "_e64",
            _ => "",
        }
    }

    fn operand(&self, name: &str, (field, ty, size): Operand) -> Option<String> {
        let f = self.format.name;
        let v = self.field(field);
        let mut count = u64::from(size.div_ceil(32).max(1));
        let text = match (field, ty) {
            ("", "VCC") => "vcc_lo".into(),
            // v_cmpx writes EXEC, which LLVM leaves out.
            ("", _) => return None,
            (_, "SIMM16") if f == "sopk" => format!("{v:#x}"),
            (_, "SIMM16") => v.to_string(),
            (_, "LABEL") => (v as u16 as i16).to_string(),
            (_, "WAITCNT") => waitcnt(v),
            (_, "HWREG") => hwreg(v),
            (_, "SIMM32") if f == "sopk" => self.literal().to_string(),
            (_, "SIMM32") => format!("{:#x}", self.literal()),
            (_, "TGT") => name_in(generated::TGT_NAMES, v),
            (_, "ATTR") => format!("attr{v}.{}", ["x", "y", "z", "w"][self.field("ATTRCHAN") as usize]),
            (_, "SMEM_OFFSET") => {
                let offset = self.field("OFFSET");
                match (v, offset) {
                    (_, 0) => register(v, 1),
                    (NULL, _) => format!("{offset:#x}"),
                    _ => format!("{} offset:{offset:#x}", register(v, 1)),
                }
            }
            (_, "SENDMSG") => sendmsg(v),
            // shortcut: printed as raw values; LLVM names these (depctr...).
            (_, "WAITCNT_DEPCTR" | "CLAUSE" | "VERSION" | "PARAM") => format!("{v:#x}"),
            (_, "VGPR") if f == "exp" => {
                let n = field.trim_start_matches("VSRC").parse::<u64>().unwrap_or(0);
                if self.field("EN") >> n & 1 == 0 { "off".into() } else { register(256 + v, 1) }
            }
            ("VADDR", _) if matches!(f, "mubuf" | "mtbuf") => match self.field("OFFEN") + self.field("IDXEN") {
                0 => "off".into(),
                n => register(256 + v, n),
            },
            ("VADDR", _) if f == "mimg" => {
                let n = image_addresses(name, self.field("DIM") as usize);
                let nsa = self.field("NSA") as usize;
                if nsa == 0 {
                    register(256 + v, n)
                } else {
                    let extra = (0..nsa * 4).map(|i| self.word(2 + i / 4) >> (i % 4 * 8) & 0xFF);
                    let all: Vec<String> = std::iter::once(v).chain(extra).take(n as usize).map(|a| register(256 + a, 1)).collect();
                    format!("[{}]", all.join(", "))
                }
            }
            ("SADDR", _) if v == NULL => "off".into(),
            _ => {
                let mut v = v;
                match field {
                    "SRSRC" | "SSAMP" => v *= 4,
                    "SBASE" => v *= 2,
                    _ => {}
                }
                if let Some(extra) = self.extra().filter(|_| field == "SRC0") {
                    let dword = self.word(1);
                    // SDWA bit 23: SRC0 is an SGPR.
                    v = (dword & 0xFF) + if extra == SDWA && dword >> 23 & 1 == 1 { 0 } else { 256 };
                } else if ty.starts_with("VGPR") {
                    v += 256;
                }
                match (f, field) {
                    // shortcut: wave32 only; a lane mask is one SGPR (wave64 would be two).
                    ("vop1" | "vop2" | "vopc" | "vop3" | "vop3p", _) if matches!(ty, "VCC" | "SREG" | "SDST") => count = 1,
                    ("mubuf" | "mtbuf", "VDATA") => count += self.field("TFE"),
                    ("mimg", "VDATA") => {
                        let n = if name.starts_with("image_gather4") { 4 } else { self.field("DMASK").count_ones().max(1) as u64 };
                        count = if self.field("D16") == 1 { n.div_ceil(2) } else { n } + (self.field("TFE") | self.field("LWE"));
                    }
                    ("mimg", "SRSRC") if self.field("R128") == 1 => count = 4,
                    ("flat_glbl" | "flat_scratch", "ADDR") if self.field("SADDR") != NULL => count = 1,
                    _ => {}
                }
                let mut text = if v == LITERAL { format!("{:#x}", self.literal()) } else { register(v, count) };
                if let Some(i) = ["SRC0", "SRC1", "SRC2"].iter().position(|s| *s == field).filter(|_| f == "vop3") {
                    // VOP3B keeps its SDST where VOP3A has ABS.
                    if self.operands.iter().all(|o| o.0 != "SDST") && self.field("ABS") >> i & 1 == 1 {
                        text = format!("|{text}|");
                    }
                    if self.field("NEG") >> i & 1 == 1 {
                        text = format!("-{text}");
                    }
                }
                text
            }
        };
        Some(text)
    }

    fn modifiers(&self, name: &str) -> Vec<String> {
        let mut m = Vec::new();
        let flag = |m: &mut Vec<String>, field: &str, text: &str| {
            if self.field(field) == 1 {
                m.push(text.to_string());
            }
        };
        let sources = self.operands.iter().filter(|o| o.0.starts_with("SRC")).count();
        let omod = |m: &mut Vec<String>, v: u64| {
            if v > 0 {
                m.push(["", "mul:2", "mul:4", "div:2"][v as usize].into());
            }
        };
        match self.format.name {
            "smem" => {
                flag(&mut m, "GLC", "glc");
                flag(&mut m, "DLC", "dlc");
            }
            "vop3" => {
                let op_sel = self.field("OP_SEL");
                if op_sel != 0 && self.operands.iter().all(|o| o.0 != "SDST") {
                    m.push(format!("op_sel:{}", bit_list(op_sel, sources + 1)));
                }
                flag(&mut m, "CLAMP", "clamp");
                omod(&mut m, self.field("OMOD"));
            }
            "vop3p" => {
                let all = (1 << sources) - 1;
                let hi = self.field("OP_SEL_HI") | self.field("OP_SEL_HI_2") << 2;
                for (text, bits, default) in [
                    ("op_sel", self.field("OP_SEL"), 0),
                    ("op_sel_hi", hi, all),
                    ("neg_lo", self.field("NEG"), 0),
                    ("neg_hi", self.field("NEG_HI"), 0),
                ] {
                    if bits & all != default {
                        m.push(format!("{text}:{}", bit_list(bits, sources)));
                    }
                }
                flag(&mut m, "CLAMP", "clamp");
            }
            "vop1" | "vop2" | "vopc" => {
                let d = self.word(1);
                match self.extra() {
                    Some(SDWA) => {
                        // shortcut: SDWA source neg/abs/sext and VOPC's SDST are not printed.
                        let sel = |s: u64| ["BYTE_0", "BYTE_1", "BYTE_2", "BYTE_3", "WORD_0", "WORD_1", "DWORD", "7"][s as usize & 7];
                        if d >> 13 & 1 == 1 {
                            m.push("clamp".into());
                        }
                        omod(&mut m, d >> 14 & 3);
                        if self.format.name != "vopc" {
                            m.push(format!("dst_sel:{}", sel(d >> 8)));
                            m.push(format!("dst_unused:{}", ["UNUSED_PAD", "UNUSED_SEXT", "UNUSED_PRESERVE", "3"][d as usize >> 11 & 3]));
                        }
                        m.push(format!("src0_sel:{}", sel(d >> 16)));
                        if self.format.name != "vop1" {
                            m.push(format!("src1_sel:{}", sel(d >> 24)));
                        }
                    }
                    Some(DPP8 | DPP8_FI) => {
                        let lanes: Vec<String> = (0..8).map(|i| (d >> (8 + 3 * i) & 7).to_string()).collect();
                        m.push(format!("dpp8:[{}]", lanes.join(",")));
                        if self.extra() == Some(DPP8_FI) {
                            m.push("fi:1".into());
                        }
                    }
                    Some(_) => {
                        // shortcut: DPP16 source neg/abs are not printed.
                        let ctrl = d >> 8 & 0x1FF;
                        m.push(match ctrl {
                            0..=0xFF => format!("quad_perm:[{},{},{},{}]", ctrl & 3, ctrl >> 2 & 3, ctrl >> 4 & 3, ctrl >> 6 & 3),
                            0x101..=0x10F => format!("row_shl:{}", ctrl & 0xF),
                            0x111..=0x11F => format!("row_shr:{}", ctrl & 0xF),
                            0x121..=0x12F => format!("row_ror:{}", ctrl & 0xF),
                            0x140 => "row_mirror".into(),
                            0x141 => "row_half_mirror".into(),
                            0x150..=0x15F => format!("row_share:{}", ctrl & 0xF),
                            0x160..=0x16F => format!("row_xmask:{}", ctrl & 0xF),
                            _ => format!("dpp_ctrl:{ctrl:#x}"),
                        });
                        m.push(format!("row_mask:{:#x}", d >> 28 & 0xF));
                        m.push(format!("bank_mask:{:#x}", d >> 24 & 0xF));
                        if d >> 19 & 1 == 1 {
                            m.push("bound_ctrl:1".into());
                        }
                        if d >> 18 & 1 == 1 {
                            m.push("fi:1".into());
                        }
                    }
                    None => {}
                }
            }
            "ds" => {
                let (o0, o1) = (self.field("OFFSET0"), self.field("OFFSET1"));
                // Dual-address ops (ds_read2*, ds_write2*, ds_wrxchg2*) take two 8-bit offsets.
                if ["ds_read2", "ds_write2", "ds_wrxchg2"].iter().any(|p| name.starts_with(p)) {
                    for (text, o) in [("offset0", o0), ("offset1", o1)] {
                        if o != 0 {
                            m.push(format!("{text}:{o}"));
                        }
                    }
                } else if o0 | o1 << 8 != 0 {
                    m.push(format!("offset:{}", o0 | o1 << 8));
                }
                flag(&mut m, "GDS", "gds");
            }
            f @ ("mubuf" | "mtbuf") => {
                if f == "mtbuf" {
                    // shortcut: only BUF_FMT_32_FLOAT is named; others print as numbers.
                    m.push(match self.field("FORMAT") {
                        22 => "format:[BUF_FMT_32_FLOAT]".into(),
                        n => format!("format:{n}"),
                    });
                }
                flag(&mut m, "IDXEN", "idxen");
                flag(&mut m, "OFFEN", "offen");
                if self.field("OFFSET") != 0 {
                    m.push(format!("offset:{}", self.field("OFFSET")));
                }
                for (field, text) in [("GLC", "glc"), ("SLC", "slc"), ("DLC", "dlc"), ("LDS", "lds"), ("TFE", "tfe")] {
                    flag(&mut m, field, text);
                }
            }
            "mimg" => {
                if self.field("DMASK") != 0 {
                    m.push(format!("dmask:{:#x}", self.field("DMASK")));
                }
                let dims = ["1D", "2D", "3D", "CUBE", "1D_ARRAY", "2D_ARRAY", "2D_MSAA", "2D_MSAA_ARRAY"];
                m.push(format!("dim:SQ_RSRC_IMG_{}", dims[self.field("DIM") as usize]));
                for field in ["UNORM", "GLC", "SLC", "DLC", "R128", "A16", "TFE", "LWE", "D16"] {
                    flag(&mut m, field, &field.to_lowercase());
                }
            }
            f @ ("flat" | "flat_glbl" | "flat_scratch") => {
                let raw = self.field("OFFSET");
                // Global and scratch offsets are signed 12-bit.
                let offset = if f == "flat" { raw as i64 } else { ((raw << 52) as i64) >> 52 };
                if offset != 0 {
                    m.push(format!("offset:{offset}"));
                }
                for (field, text) in [("GLC", "glc"), ("SLC", "slc"), ("DLC", "dlc"), ("LDS", "lds")] {
                    flag(&mut m, field, text);
                }
            }
            "exp" => {
                for (field, text) in [("DONE", "done"), ("COMPR", "compr"), ("VM", "vm")] {
                    flag(&mut m, field, text);
                }
            }
            _ => {}
        }
        m
    }
}

/// The instruction as `llvm-objdump` prints it, e.g. `v_add_f32_e32 v0, v1, v2`.
impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Some(name) = self.name else {
            return write!(f, "<unknown {} opcode {}>", self.format.name, self.opcode);
        };
        write!(f, "{name}{}", self.suffix(name))?;
        let mut operands: Vec<String> = self.operands.iter().filter_map(|&o| self.operand(name, o)).collect();
        if self.format.name == "exp" && !operands.is_empty() {
            // `exp mrt0 v0, ...`: no comma after the target.
            let target = operands.remove(0);
            write!(f, " {target}")?;
        }
        if !operands.is_empty() {
            write!(f, " {}", operands.join(", "))?;
        }
        for m in self.modifiers(name) {
            write!(f, " {m}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(words: &[u32]) -> (&'static str, Option<&'static str>, usize) {
        let i = decode(words).unwrap();
        (i.format.name, i.name, i.dwords)
    }

    #[test]
    fn known_instructions() {
        assert_eq!(name(&[0xBF81_0000]), ("sopp", Some("s_endpgm"), 1));
        assert_eq!(name(&[0xBE80_0301]), ("sop1", Some("s_mov_b32"), 1));
        assert_eq!(name(&[0xB000_0001]), ("sopk", Some("s_movk_i32"), 1));
        assert_eq!(name(&[0x8000_0201]), ("sop2", Some("s_add_u32"), 1));
        assert_eq!(name(&[0x7E00_0301]), ("vop1", Some("v_mov_b32"), 1));
        assert_eq!(name(&[0x0600_0501]), ("vop2", Some("v_add_f32"), 1));
        // VOP2 opcodes in VOP3 form sit at 256 + opcode; VOP2 has no 0.
        assert_eq!(name(&[0xD503_0000, 0x0002_0501]), ("vop3", Some("v_add_f32"), 2));
        assert_eq!(name(&[0xD500_0000, 0]), ("vop3", None, 2));
    }

    #[test]
    fn trailing_dwords_are_counted() {
        // s_mov_b32 s0, 0x12345678: SSRC0 = 255.
        assert_eq!(name(&[0xBE80_03FF, 0x1234_5678]), ("sop1", Some("s_mov_b32"), 2));
        // v_fmamk_f32 always carries its constant.
        assert_eq!(name(&[0x5800_0000, 0]), ("vop2", Some("v_fmamk_f32"), 2));
        // v_mov_b32 with DPP16 (SRC0 = 0xFA).
        assert_eq!(name(&[0x7E00_02FA, 0]), ("vop1", Some("v_mov_b32"), 2));
        // Global and scratch loads are told apart by SEG.
        assert_eq!(decode(&[0xDC00_8000, 0]).unwrap().format.name, "flat_glbl");
        assert_eq!(decode(&[0xDC00_4000, 0]).unwrap().format.name, "flat_scratch");
    }

    /// llvm-objdump's text and dwords for tests/asm/formats.s (LLVM 18, gfx1030, wave32).
    const LLVM: &str = "\
s_mov_b32 s0, s1|BE800301
s_mov_b32 s0, 0x12345678|BE8003FF 12345678
s_add_u32 s0, s1, s2|80000201
s_add_u32 s0, s1, 0x12345678|8000FF01 12345678
s_movk_i32 s0, 0x1234|B0001234
s_cmp_eq_u32 s0, s1|BF060100
s_setreg_imm32_b32 hwreg(HW_REG_MODE), 0|BA80F801 00000000
s_load_dword s0, s[2:3], null|F4000001 FA000000
v_mov_b32_e32 v0, v1|7E000301
v_mov_b32_e32 v0, 0x3f800001|7E0002FF 3F800001
v_add_f32_e32 v0, v1, v2|06000501
v_add_f32_e64 v0, v1, v2|D5030000 00020501
v_cmp_eq_f32_e32 vcc_lo, v0, v1|7C040300
v_cndmask_b32_e32 v0, v1, v2, vcc_lo|02000501
v_fmamk_f32 v0, v1, 0x40000000, v2|58000501 40000000
v_fmaak_f32 v0, v1, v2, 0x40000000|5A000501 40000000
v_fma_f32 v0, v1, v2, v3|D54B0000 040E0501
v_fma_f32 v0, v1, v2, 2.0|D54B0000 03D20501
v_pk_add_f16 v0, v1, v2|CC0F4000 18020501
v_mov_b32_dpp v0, v1 quad_perm:[1,0,3,2] row_mask:0xf bank_mask:0xf|7E0002FA FF00B101
v_mov_b32_dpp v0, v1 dpp8:[7,6,5,4,3,2,1,0]|7E0002E9 05397701
v_mov_b32_sdwa v0, v1 dst_sel:WORD_1 dst_unused:UNUSED_PAD src0_sel:WORD_0|7E0002F9 00040501
v_interp_p1_f32_e32 v0, v1, attr0.x|C8000001
ds_read_b32 v0, v1|D8D80000 00000001
buffer_load_dword v0, v1, s[4:7], 0 offen|E0301000 80010001
tbuffer_load_format_x v0, v1, s[4:7], 0 format:[BUF_FMT_32_FLOAT] offen|E8B01000 80010001
image_sample v[0:3], v[4:5], s[8:15], s[16:19] dmask:0xf dim:SQ_RSRC_IMG_2D|F0800F08 00820004
image_sample v[0:3], [v4, v6], s[8:15], s[16:19] dmask:0xf dim:SQ_RSRC_IMG_2D|F0800F0A 00820004 00000006
flat_load_dword v0, v[2:3]|DC300000 007D0002
global_load_dword v0, v[2:3], off|DC308000 007D0002
scratch_load_dword v0, v1, off|DC304000 007D0001
exp mrt0 v0, v1, v2, v3 done vm|F800180F 03020100
s_nop 0|BF800000
s_branch 2|BF820002
s_cbranch_execz 1|BF880001
s_waitcnt vmcnt(0) lgkmcnt(0)|BF8C0070
s_load_dwordx2 s[0:1], s[2:3], 0x10|F4040001 FA000010
v_mov_b32_e32 v0, -1|7E0002C1
v_add_f32_e32 v0, 0.5, v1|060002F0
v_add_co_u32 v0, vcc_lo, v1, v2|D70F6A00 00020501
v_cmp_eq_f32_e64 s0, v0, v1|D4020000 00020300
v_fma_f32 v0, -v1, |v2|, v3 clamp|D54B8200 240E0501
v_mul_f32_e64 v0, v1, v2 mul:2|D5080000 08020501
ds_write_b32 v1, v0 offset:4|D8340004 00000001
buffer_store_dword v0, v1, s[4:7], 0 offen offset:16 glc|E0705010 80010001
global_load_dwordx2 v[0:1], v[2:3], off offset:8|DC348008 007D0002
image_load v[0:3], v[0:1], s[8:15] dmask:0xf dim:SQ_RSRC_IMG_2D|F0000F08 00020000
s_endpgm|BF810000";

    #[test]
    fn text_matches_llvm_objdump() {
        let mut wrong = Vec::new();
        for line in LLVM.lines() {
            let (text, hex) = line.rsplit_once('|').unwrap();
            let words: Vec<u32> = hex.split(' ').map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
            let i = decode(&words).unwrap();
            assert_eq!(i.dwords, words.len(), "{text}");
            let ours = i.to_string();
            if ours != text {
                wrong.push(format!("llvm: {text}\nours: {ours}"));
            }
        }
        assert_eq!(LLVM.lines().count(), 48);
        assert!(wrong.is_empty(), "{} of 48 differ:\n{}", wrong.len(), wrong.join("\n"));
    }
}
