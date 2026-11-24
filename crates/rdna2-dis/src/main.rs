use std::process::ExitCode;

use rdna2_isa::{analysis, blocks, disassemble, elf, name_targets};

const USAGE: &str = "usage: rdna2-dis [--wave64] [--dot | --live] <code.o | code.hsaco | code.bin>

Disassembles AMD RDNA2 (gfx10.3) machine code in llvm-objdump's spelling.
Takes an ELF code object (its .text section) or a raw code blob.

  --wave64  print lane masks for 64-lane waves (vcc, s[0:1])
  --dot     print the control-flow graph in Graphviz DOT instead
  --live    print live VGPRs and SGPRs and the EXEC region at each instruction";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let unknown = args.iter().find(|a| a.starts_with("--") && !["--wave64", "--dot", "--live"].contains(&a.as_str()));
    let ([path], None) = (paths.as_slice(), unknown) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let file = match std::fs::read(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let code = match elf::code(&file) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let wave64 = flag("--wave64");
    let mut lines = disassemble(code.bytes, wave64);
    name_targets(&mut lines, &code.symbols);
    let entries: Vec<usize> = code.symbols.iter().map(|s| s.0).collect();
    if flag("--dot") {
        print!("{}", dot(&lines, &blocks(&lines, &entries)));
        return ExitCode::SUCCESS;
    }
    if flag("--live") {
        print!("{}", live(&lines, &blocks(&lines, &entries), wave64));
        return ExitCode::SUCCESS;
    }
    for line in &lines {
        for (_, name) in code.symbols.iter().filter(|s| s.0 == line.offset) {
            println!("\n<{name}>:");
        }
        let hex: Vec<String> = line.words.iter().map(|w| format!("{w:08X}")).collect();
        println!("{:08x}: {:<60} // {}", line.offset, line.text, hex.join(" "));
    }
    ExitCode::SUCCESS
}

/// The control-flow graph as Graphviz DOT, one box per basic block.
fn dot(lines: &[rdna2_isa::Line], blocks: &[rdna2_isa::Block]) -> String {
    let mut out = String::from("digraph cfg {\n  node [shape=box fontname=monospace];\n");
    for (i, block) in blocks.iter().enumerate() {
        let body: String = lines[block.lines.clone()]
            .iter()
            // `\l` ends a left-aligned line in a DOT label.
            .map(|l| format!("{:08x}  {}\\l", l.offset, l.text.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        out += &format!("  b{i} [label=\"{body}\"];\n");
        for s in &block.successors {
            out += &format!("  b{i} -> b{s};\n");
        }
    }
    out + "}\n"
}

/// Each instruction with the VGPRs and SGPRs live before it and the writes
/// of EXEC that reach it, then the peaks.
fn live(lines: &[rdna2_isa::Line], blocks: &[rdna2_isa::Block], wave64: bool) -> String {
    let a = analysis::analyze(lines, blocks, wave64);
    let mut out = format!("{:<8}  {:>5} {:>5}  {:<16}  assembly
", "offset", "vgprs", "sgprs", "exec");
    for (i, line) in lines.iter().enumerate() {
        let exec = analysis::exec_label(lines, &a.exec[i]);
        out += &format!("{:08x}  {:>5} {:>5}  {exec:<16}  {}
", line.offset, a.live[i].vgprs(), a.live[i].sgprs(), line.text);
    }
    let peak = |f: fn(&analysis::Regs) -> u32| a.live.iter().map(f).max().unwrap_or(0);
    out += &format!("
peak live: {} VGPRs, {} SGPRs
", peak(analysis::Regs::vgprs), peak(analysis::Regs::sgprs));
    if let Some(entry) = a.live.first() {
        out += &format!("live on entry: {}
", entry.describe());
    }
    out
}
