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
    for (offset, text, words) in rdna2_isa::disassemble(&bytes) {
        let hex: Vec<String> = words.iter().map(|w| format!("{w:08X}")).collect();
        println!("{offset:08x}: {text:<60} // {}", hex.join(" "));
    }
    ExitCode::SUCCESS
}
