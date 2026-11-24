use std::process::ExitCode;

/// Disassembles a raw code blob: `offset: assembly  // dwords`, in
/// `llvm-objdump`'s spelling.
fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: rdna2-dis <code.bin>");
        return ExitCode::FAILURE;
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let words: Vec<u32> = bytes.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
    let mut i = 0;
    while i < words.len() {
        let decoded = rdna2_isa::decode(&words[i..]);
        let n = decoded.as_ref().map_or(1, |d| d.dwords.min(words.len() - i));
        let text = decoded.map_or("<unknown format>".into(), |d| d.to_string());
        let hex: Vec<String> = words[i..i + n].iter().map(|w| format!("{w:08X}")).collect();
        println!("{:08x}: {text:<60} // {}", i * 4, hex.join(" "));
        i += n;
    }
    ExitCode::SUCCESS
}
