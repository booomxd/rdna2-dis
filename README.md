# rdna2-dis

A disassembler and visualizer for AMD RDNA2 (gfx10.3) GPU machine code, written in Rust from AMD's public ISA documentation. Clean-room: no vendor or game binaries are included.

## Status

The disassembler works. Every RDNA2 instruction format, field, opcode and operand list (about 1,100 instructions) is generated from AMD's machine-readable ISA specification. Output is real assembly in `llvm-objdump`'s spelling: registers and ranges, inline and literal constants, VOP3/VOP3P modifiers, DPP16/DPP8/SDWA, memory offsets and flags, image `dmask`/`dim` with NSA addresses, export targets, `s_waitcnt` and branches.

```
00000000: v_fma_f32 v0, -v1, |v2|, v3 clamp                          // D54B8200 240E0501
00000008: buffer_store_dword v0, v1, s[4:7], 0 offen offset:16 glc    // E0705010 80010001
00000010: image_sample v[0:3], [v4, v6], s[8:15], s[16:19] dmask:0xf dim:SQ_RSRC_IMG_2D // F0800F0A 00820004 00000006
```

A unit test checks 48 instructions covering every format against `llvm-objdump`'s text, and CI checks instruction boundaries against LLVM on every push. Wave32 only.

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
- [x] Operands and modifiers, in `llvm-objdump`'s spelling
- [x] Instruction boundaries checked against `llvm-objdump` in CI
- [ ] Full-line comparison against `llvm-objdump` in CI
- [ ] Control-flow graph, register liveness, EXEC-mask regions
- [ ] Web UI (WASM)

## License

MIT
