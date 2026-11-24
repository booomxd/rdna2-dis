//! Decoding of AMD RDNA2 (gfx10.3) machine code. Formats, fields and
//! opcodes are generated from AMD's machine-readable ISA specification
//! (`cargo run -p xtask`).

mod generated;

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
    /// Opcode, mnemonic, and whether a literal dword always follows.
    opcodes: &'static [(u32, &'static str, bool)],
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
}

/// Source operand values that mean "a constant dword follows".
const LITERAL: u64 = 255;
/// VOP1/VOP2/VOPC SRC0 values selecting a DPP8, DPP8 FI, SDWA or DPP16
/// dword after the instruction.
const EXTRA_DWORD: [u64; 4] = [0xE9, 0xEA, 0xF9, 0xFA];

#[derive(Debug)]
pub struct Instruction {
    pub format: &'static Spec,
    pub opcode: u32,
    /// `None` for an opcode the format does not define.
    pub name: Option<&'static str>,
    /// Dwords including literal, DPP/SDWA and address dwords.
    pub dwords: usize,
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
    Some(Instruction { format, opcode, name: entry.map(|o| o.1), dwords })
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
}
