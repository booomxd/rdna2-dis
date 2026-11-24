//! The disassembler for the web page in `web/`.

use wasm_bindgen::prelude::wasm_bindgen;

/// One line per instruction: `offset<TAB>assembly<TAB>dwords in hex`.
#[wasm_bindgen]
pub fn disassemble(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (offset, text, words) in rdna2_isa::disassemble(bytes) {
        let hex: Vec<String> = words.iter().map(|w| format!("{w:08X}")).collect();
        out += &format!("{offset:08x}\t{text}\t{}\n", hex.join(" "));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn lines_are_tab_separated() {
        // s_mov_b32 s0, s1; s_endpgm
        let bytes = [0x01, 0x03, 0x80, 0xBE, 0x00, 0x00, 0x81, 0xBF];
        assert_eq!(super::disassemble(&bytes), "00000000\ts_mov_b32 s0, s1\tBE800301\n00000004\ts_endpgm\tBF810000\n");
    }
}
