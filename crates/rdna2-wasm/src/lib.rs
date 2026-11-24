//! The disassembler for the web page in `web/`.

use wasm_bindgen::prelude::wasm_bindgen;

/// Disassembles an ELF code object (its `.text`) or a raw code blob into
/// tab-separated records, one per line:
///
/// - `S  offset  name`: a symbol
/// - `I  offset  assembly  dwords`: an instruction (offset and dwords in hex)
/// - `B  first  end  successors`: a basic block, as instruction indices
///   `first..end`, and the indices of the blocks it can go to (comma-separated)
/// - `E  message`: the file could not be read
#[wasm_bindgen]
pub fn listing(file: &[u8], wave64: bool) -> String {
    let code = match rdna2_isa::elf::code(file) {
        Ok(code) => code,
        Err(e) => return format!("E\t{e}\n"),
    };
    let lines = rdna2_isa::disassemble(code.bytes, wave64);
    let mut out = String::new();
    for (offset, name) in &code.symbols {
        out += &format!("S\t{offset:x}\t{name}\n");
    }
    for line in &lines {
        let hex: Vec<String> = line.words.iter().map(|w| format!("{w:08X}")).collect();
        out += &format!("I\t{:x}\t{}\t{}\n", line.offset, line.text, hex.join(" "));
    }
    let entries: Vec<usize> = code.symbols.iter().map(|s| s.0).collect();
    for block in rdna2_isa::blocks(&lines, &entries) {
        let successors: Vec<String> = block.successors.iter().map(usize::to_string).collect();
        out += &format!("B\t{}\t{}\t{}\n", block.lines.start, block.lines.end, successors.join(","));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn records() {
        // s_cbranch_execz 0 (to 4); s_endpgm
        let bytes = [0x00, 0x00, 0x88, 0xBF, 0x00, 0x00, 0x81, 0xBF];
        assert_eq!(
            super::listing(&bytes, false),
            "I\t0\ts_cbranch_execz 0\tBF880000\nI\t4\ts_endpgm\tBF810000\nB\t0\t1\t1\nB\t1\t2\t\n"
        );
        assert!(super::listing(b"\x7fELF\x01\x01", false).starts_with("E\t"));
    }
}
