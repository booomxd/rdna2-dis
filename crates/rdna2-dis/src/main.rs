use std::process::ExitCode;

use rdna2_isa::Encoding;

/// Lists a raw code blob instruction by instruction: offset, format, dwords.
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
        let encoding = Encoding::of(words[i]);
        let n = encoding.map_or(1, |e| e.dwords().min(words.len() - i));
        let name = encoding.map_or("unknown".into(), |e| format!("{e:?}"));
        let hex: Vec<String> = words[i..i + n].iter().map(|w| format!("{w:08x}")).collect();
        println!("{:08x}: {name:<7} {}", i * 4, hex.join(" "));
        i += n;
    }
    ExitCode::SUCCESS
}
