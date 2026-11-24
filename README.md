# rdna2-dis

[![CI](https://github.com/booomxd/rdna2-dis/actions/workflows/ci.yml/badge.svg)](https://github.com/booomxd/rdna2-dis/actions/workflows/ci.yml)
[![Live demo](https://github.com/booomxd/rdna2-dis/actions/workflows/pages.yml/badge.svg)](https://booomxd.github.io/rdna2-dis/)
[![Release](https://img.shields.io/github/v/release/booomxd/rdna2-dis)](https://github.com/booomxd/rdna2-dis/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A disassembler and control-flow visualizer for AMD RDNA2 (gfx10.3) GPU machine code, written in Rust. It decodes every RDNA2 instruction format, prints assembly exactly as `llvm-objdump` does, and builds the control-flow graph of a shader. It runs as a command-line tool and, compiled to WebAssembly, in the browser.

**[Try it in your browser →](https://booomxd.github.io/rdna2-dis/)** Paste hex or drop a code object. Nothing is uploaded.

![The web UI showing a shader's control-flow graph](docs/screenshot.jpg)

## Features

- **Complete instruction coverage.** All RDNA2 encodings (SOP1/SOP2/SOPC/SOPK/SOPP, SMEM, VOP1/VOP2/VOPC/VOP3/VOP3P, VINTRP, DS, MUBUF/MTBUF, MIMG, EXP, FLAT/GLOBAL/SCRATCH) and about 1,100 opcodes, generated from AMD's machine-readable ISA specification rather than typed by hand.
- **LLVM-identical output.** Registers and ranges, inline and literal constants, VOP3/VOP3P modifiers, DPP16/DPP8/SDWA, memory offsets and cache flags, MIMG `dmask`/`dim` with NSA addresses, export targets, and named operands: `s_waitcnt`, `hwreg(...)`, `sendmsg(...)`, `s_waitcnt_depctr` fields and buffer formats.
- **Verified against LLVM.** CI assembles test programs with `llvm-mc` and fails if any line `rdna2-dis` prints differs from `llvm-objdump`.
- **Control-flow graph.** Splits code into basic blocks at branches and branch targets, with taken, fall-through and back edges. Exported as Graphviz DOT from the CLI and drawn in the web UI.
- **Register liveness and EXEC regions.** Which VGPRs and SGPRs hold a value at each instruction, and peak register pressure, which limits how many waves fit on a compute unit. Shows which write of EXEC each instruction runs under (an `if` that turns lanes off, the restore after it, a `v_cmpx`), so divergent code stands out. Read and write sets come from AMD's specification, including the registers an instruction does not print, such as the SCC that `s_add_u32` sets.
- **Reads real code objects.** Takes AMDGPU ELF files (`.o`, `.hsaco`) and disassembles their `.text` section with symbol labels and named branch targets, or raw code blobs.
- **Wave32 and wave64.** Lane masks print as `vcc_lo`/`s0` or `vcc`/`s[0:1]`.
- **Small.** The decoder crate has no dependencies, and the WebAssembly build is under 200 KB.

## Example

```
$ rdna2-dis shader.bin
00000000: s_load_dword s0, s[2:3], null                                // F4000001 FA000000
00000008: s_waitcnt vmcnt(0) lgkmcnt(0)                                // BF8C0070
0000000c: v_fma_f32 v0, -v1, |v2|, v3 clamp                            // D54B8200 240E0501
00000014: buffer_store_dword v0, v1, s[4:7], 0 offen offset:16 glc     // E0705010 80010001
0000001c: image_sample v[0:3], [v4, v6], s[8:15], s[16:19] dmask:0xf dim:SQ_RSRC_IMG_2D // F0800F0A 00820004 00000006
00000028: s_endpgm                                                     // BF810000
```

## Installation

Download a prebuilt binary for Linux, Windows or macOS from the [latest release](https://github.com/booomxd/rdna2-dis/releases/latest), or build from source with a recent stable Rust toolchain:

```
cargo install --git https://github.com/booomxd/rdna2-dis rdna2-dis
```

## Usage

```
rdna2-dis [--wave64] [--dot | --live] <code.o | code.hsaco | code.bin>
```

| Option | Effect |
|---|---|
| `--wave64` | Print lane masks for 64-lane waves (`vcc`, `s[0:1]`). The default is wave32. |
| `--dot` | Print the control-flow graph in Graphviz DOT instead of the listing. |
| `--live` | Print the registers live before each instruction and the EXEC region it runs in. |

Render a control-flow graph with Graphviz:

```
rdna2-dis --dot shader.o | dot -Tsvg -o cfg.svg
```

See register pressure and divergence:

```
$ rdna2-dis --live shader.bin
offset    vgprs sgprs  exec              assembly
00000000      2     0  entry             v_cmp_eq_f32_e32 vcc_lo, v0, v1
00000004      2     0  entry             s_and_saveexec_b32 s0, vcc_lo
00000008      2     1  0x4 if            s_cbranch_execz 1
0000000c      2     1  0x4 if            v_add_f32_e32 v2, v0, v1
00000010      0     1  0x4 if            s_or_b32 exec_lo, exec_lo, s0
00000014      0     0  0x10 restore      s_endpgm

peak live: 2 VGPRs, 1 SGPRs
live on entry: v[0:1], exec_lo
```

`s0` stays live from the `s_and_saveexec_b32` that saves the mask to the `s_or_b32` that restores it, and the instructions between run with only the lanes where `v0 == v1`.

![Liveness and EXEC regions in the web UI](docs/liveness.jpg)

To get a code object, compile a kernel for an RDNA2 target, for example `clang -target amdgcn-amd-amdhsa -mcpu=gfx1030 -c kernel.cl`.

### As a library

The `rdna2-isa` crate holds the decoder:

```rust
let code = rdna2_isa::elf::code(&file)?;                 // .text of an ELF, or the raw bytes
let mut lines = rdna2_isa::disassemble(code.bytes, false); // wave32
rdna2_isa::name_targets(&mut lines, &code.symbols);       // `s_branch done`
let cfg = rdna2_isa::blocks(&lines, &[]);                 // basic blocks and their edges
let a = rdna2_isa::analysis::analyze(&lines, &cfg, false); // liveness and EXEC regions
for (i, line) in lines.iter().enumerate() {
    println!("{:08x}: {:<50} live: {}", line.offset, line.text, a.live[i].describe());
}
```

## How it works

```
AMD ISA XML ──xtask──▶ generated.rs ──▶ rdna2-isa ──┬──▶ rdna2-dis   (CLI)
(GPUOpen, MIT)         formats, opcodes, decode,     └──▶ rdna2-wasm  ──▶ web/ (GitHub Pages)
                       operand lists     print, CFG,
                                         liveness
```

| Path | Contents |
|---|---|
| `xtask/` | Generator that reads AMD's ISA XML and writes the encoding and opcode tables. |
| `crates/rdna2-isa/` | Decoder: format detection, instruction length, operand printing, control flow, liveness, ELF reading. |
| `crates/rdna2-dis/` | Command-line tool. |
| `crates/rdna2-wasm/` | WebAssembly bindings for the web UI. |
| `web/` | The web UI: plain HTML, CSS and JavaScript, with no framework or bundler. |
| `tests/asm/` | Assembly programs that CI checks against `llvm-objdump`. |

Decoding is table-driven. Each encoding is identified by its fixed bits; where encodings overlap, the most specific mask wins. An instruction's length follows from its format and operands: literal constants, DPP and SDWA control words, and extra MIMG address dwords. Operand types and order come from the specification, and the printing follows LLVM's conventions.

## Testing

```
cargo test --workspace
bash tests/llvm/check.sh   # needs llvm-mc and llvm-objdump with the AMDGPU backend
```

- Unit tests cover decoding, instruction lengths, operand names, control-flow construction, register read and write sets, liveness, EXEC regions, ELF parsing (including malformed and truncated files) and the WebAssembly interface.
- `tests/llvm/check.sh` assembles every file in `tests/asm/` with `llvm-mc` for gfx1030, disassembles the object with both `llvm-objdump` and `rdna2-dis`, and diffs every instruction and label line. Files named `*.wave64.s` are checked in wave64 mode. CI runs it on Ubuntu, whose LLVM packages include the AMDGPU backend.

## Limitations

- RDNA2 (gfx10.3) only. RDNA1 and RDNA3 use different encodings.
- Liveness treats a VGPR write as replacing the whole register, as most tools do, so a value kept in lanes EXEC turned off is not tracked. Calls (`s_swappc_b64`) are assumed to return without touching registers.
- MIMG address counts assume 32-bit addresses; 16-bit addresses (A16/G16) are not accounted for.
- DPP16 and SDWA source modifiers (`neg`, `abs`, `sext`) are not printed.

## Regenerating the tables

Download AMD's machine-readable ISA specification from [GPUOpen](https://gpuopen.com/machine-readable-isa/), then:

```
cargo run -p xtask -- path/to/amdgpu_isa_rdna2.xml
```

This rewrites `crates/rdna2-isa/src/generated.rs`.

## Building the web UI

```
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
cargo build -p rdna2-wasm --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/rdna2_wasm.wasm
python -m http.server -d web
```

Every push to `main` deploys it to GitHub Pages.

## Roadmap

- [x] Opcode tables generated from AMD's machine-readable specification
- [x] Operands and modifiers in `llvm-objdump`'s spelling, checked line by line in CI
- [x] ELF code objects with symbols
- [x] Wave32 and wave64
- [x] Control-flow graph (CLI and web)
- [x] Web UI and live demo
- [x] Prebuilt releases
- [x] Register liveness and EXEC-mask regions
- [ ] RDNA3 (gfx11)

## License

MIT, see [LICENSE](LICENSE). The instruction tables are generated from AMD's machine-readable ISA specification, © Advanced Micro Devices, Inc., also under the MIT license. The repository contains no AMD or third-party binaries.
