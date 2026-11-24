use std::process::ExitCode;

/// Lists a raw code blob instruction by instruction: offset, mnemonic,
/// format and dwords.
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
        let label = match &decoded {
            Some(d) => format!("{:<24} {}", d.name.unwrap_or("<unknown opcode>"), d.format.name),
            None => "<unknown format>".into(),
        };
        let hex: Vec<String> = words[i..i + n].iter().map(|w| format!("{w:08x}")).collect();
        println!("{:08x}: {label:<36} {}", i * 4, hex.join(" "));
        i += n;
    }
    ExitCode::SUCCESS
}
