#!/usr/bin/env bash
# Assembles each tests/asm/*.s for gfx1030 with LLVM and checks that our
# disassembler prints every line exactly as llvm-objdump does. Needs
# llvm-mc and llvm-objdump with the AMDGPU backend.
set -euo pipefail

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build -q -p rdna2-dis
status=0

# Assembly text only, with runs of spaces and tabs squeezed to one space.
squeeze() { sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//'; }

# Each file is assembled into an ELF object, which both disassemblers read.
# Instruction lines and symbol labels (`<name>:`) are compared. Files named
# *.wave64.s are assembled and disassembled for 64-lane waves.
for src in tests/asm/*.s; do
    name=$(basename "$src" .s)
    if [[ $name == *.wave64 ]]; then wave=64; ours=(--wave64); else wave=32; ours=(); fi
    attr=+wavefrontsize$wave
    llvm-mc --triple=amdgcn -mcpu=gfx1030 -mattr=$attr -filetype=obj "$src" -o "$work/$name.o"
    llvm-objdump -d --mcpu=gfx1030 --mattr=$attr "$work/$name.o" > "$work/$name.dis"
    # The full listing, for reference in the log.
    grep -E '// [0-9A-Fa-f]+:|^[0-9a-f]+ <' "$work/$name.dis"
    # LLVM names sections without symbols (`<.text>:`); we don't.
    grep -E '// [0-9A-Fa-f]+:|^[0-9a-f]+ <[^.]' "$work/$name.dis" \
        | sed -E 's|^[0-9a-f]+ (<.*>:)$|\1|; s|//.*||' | squeeze > "$work/$name.llvm"
    target/debug/rdna2-dis "${ours[@]}" "$work/$name.o" | grep -E '^[0-9a-f]+: |^<' \
        | sed -E 's|^[0-9a-f]+: ||; s|//.*||' | squeeze > "$work/$name.ours"
    if diff -u "$work/$name.llvm" "$work/$name.ours"; then
        echo "$name: $(wc -l < "$work/$name.llvm") lines match"
    else
        status=1
    fi
done
exit $status
