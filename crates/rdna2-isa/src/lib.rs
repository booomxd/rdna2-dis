//! Decoding of AMD RDNA2 (gfx10.3) machine code, from AMD's public ISA
//! reference.

/// An instruction format, told apart by the high bits of the first dword.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Sop1,
    Sopc,
    Sopp,
    Sopk,
    Sop2,
    Vop1,
    Vopc,
    Vop2,
    Vop3,
    Vop3p,
    Vintrp,
    Smem,
    Ds,
    Flat,
    Mubuf,
    Mtbuf,
    Mimg,
    Exp,
}

use Encoding::*;

/// (shift, prefix, encoding), longest prefixes first: SOP1/SOPC/SOPP start
/// with SOPK's bits, and VOP1/VOPC with VOP2's.
const PREFIXES: [(u32, u32, Encoding); 18] = [
    (23, 0x17D, Sop1),
    (23, 0x17E, Sopc),
    (23, 0x17F, Sopp),
    (25, 0x3F, Vop1),
    (25, 0x3E, Vopc),
    (26, 0x32, Vintrp),
    (26, 0x33, Vop3p),
    (26, 0x35, Vop3),
    (26, 0x36, Ds),
    (26, 0x37, Flat),
    (26, 0x38, Mubuf),
    (26, 0x3A, Mtbuf),
    (26, 0x3C, Mimg),
    (26, 0x3D, Smem),
    (26, 0x3E, Exp),
    (28, 0xB, Sopk),
    (30, 0b10, Sop2),
    (31, 0, Vop2),
];

impl Encoding {
    /// The format of the instruction whose first dword is `word`.
    pub fn of(word: u32) -> Option<Encoding> {
        PREFIXES
            .iter()
            .find(|&&(shift, prefix, _)| word >> shift == prefix)
            .map(|&(_, _, encoding)| encoding)
    }

    /// Dwords of the instruction without trailing data.
    // shortcut: a literal constant (source 255) and MIMG's extra address
    // dwords (NSA) are not counted; needs operand decoding from the spec.
    pub fn dwords(self) -> usize {
        match self {
            Vop3 | Vop3p | Smem | Ds | Flat | Mubuf | Mtbuf | Mimg | Exp => 2,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_from_known_instructions() {
        for (word, encoding) in [
            (0xBF81_0000, Sopp), // s_endpgm
            (0xBE80_0301, Sop1), // s_mov_b32 s0, s1
            (0xB000_0001, Sopk), // s_movk_i32 s0, 1
            (0x8000_0201, Sop2), // s_add_u32 s0, s1, s2
            (0x7E00_0301, Vop1), // v_mov_b32 v0, v1
            (0x7C00_0301, Vopc), // v_cmp_f_f32 vcc, v1, v0
            (0x0600_0501, Vop2), // v_add_f32 v0, v1, v2
            (0xF400_0000, Smem), // s_load_dword
            (0xD500_0000, Vop3),
        ] {
            assert_eq!(Encoding::of(word), Some(encoding), "{word:#010x}");
        }
        assert_eq!(Encoding::of(0xD000_0000), None);
    }
}
