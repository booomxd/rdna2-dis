#!/usr/bin/env bash
# Assembles each tests/asm/*.s for gfx1030 with LLVM and checks that our
# disassembler prints every line exactly as llvm-objdump does. Needs
# llvm-mc, llvm-objcopy and llvm-objdump with AMDGPU.
set -euo pipefail

mc=(llvm-mc --triple=amdgcn -mcpu=gfx1030 -mattr=+wavefrontsize32)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build -q -p rdna2-dis
status=0

# Assembly text only, with runs of spaces and tabs squeezed to one space.
squeeze() { sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//'; }

for src in tests/asm/*.s; do
    name=$(basename "$src" .s)
    "${mc[@]}" -filetype=obj "$src" -o "$work/$name.o"
    llvm-objcopy -O binary --only-section=.text "$work/$name.o" "$work/$name.bin"
    llvm-objdump -d --mcpu=gfx1030 --mattr=+wavefrontsize32 "$work/$name.o" > "$work/$name.dis"
    # The full listing, for reference in the log.
    grep -E '// [0-9A-Fa-f]+:' "$work/$name.dis"
    grep -E '// [0-9A-Fa-f]+:' "$work/$name.dis" | sed -E 's|//.*||' | squeeze > "$work/$name.llvm"
    target/debug/rdna2-dis "$work/$name.bin" | sed -E 's|^[0-9a-f]+: ||; s|//.*||' | squeeze > "$work/$name.ours"
    if diff -u "$work/$name.llvm" "$work/$name.ours"; then
        echo "$name: $(wc -l < "$work/$name.llvm") lines match"
    else
        status=1
    fi
done
exit $status
