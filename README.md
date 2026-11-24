# rdna2-dis

A disassembler and visualizer for AMD RDNA2 (gfx10.3) GPU machine code, written in Rust from AMD's public ISA documentation. Clean-room: no vendor or game binaries are included.

## Status

Early. The decoder tells instruction formats apart (SOP*, VOP*, SMEM, DS, FLAT, MUBUF, MTBUF, MIMG, EXP, VINTRP). Opcode names and operands come next, generated from AMD's machine-readable ISA spec and checked against `llvm-objdump`.

## Usage

```
cargo run -p rdna2-dis -- code.bin
```

## Roadmap

- [x] Format detection
- [ ] Opcode tables generated from AMD's machine-readable spec
- [ ] Operands and literal constants
- [ ] Output checked against `llvm-objdump` in CI
- [ ] Control-flow graph, register liveness, EXEC-mask regions
- [ ] Web UI (WASM)

## License

MIT
