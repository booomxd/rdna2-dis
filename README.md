# rdna2-dis

A disassembler and visualizer for AMD RDNA2 (gfx10.3) GPU machine code, written in Rust from AMD's public ISA documentation. Clean-room: no vendor or game binaries are included.

## Status

Early. Every RDNA2 instruction format, field and opcode (about 1,100 instructions) is generated from AMD's machine-readable ISA specification. The decoder names each instruction and works out its length, including literal constants, DPP/SDWA dwords and MIMG address dwords. Operands come next, checked against `llvm-objdump`.

## Regenerating the tables

Download AMD's machine-readable ISA specification from [GPUOpen](https://gpuopen.com/machine-readable-isa/), then:

```
cargo run -p xtask -- path/to/amdgpu_isa_rdna2.xml
```

This rewrites `crates/rdna2-isa/src/generated.rs`. The specification is © Advanced Micro Devices, Inc., under the MIT license.

## Usage

```
cargo run -p rdna2-dis -- code.bin
```

## Roadmap

- [x] Format detection
- [x] Opcode tables generated from AMD's machine-readable spec
- [x] Instruction length (literals, DPP/SDWA, MIMG NSA)
- [ ] Operands
- [ ] Output checked against `llvm-objdump` in CI
- [ ] Control-flow graph, register liveness, EXEC-mask regions
- [ ] Web UI (WASM)

## License

MIT
