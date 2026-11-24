#!/usr/bin/env bash
# Assembles each tests/asm/*.s for gfx1030 with LLVM and checks that our
# disassembler finds the same instructions (mnemonic sequence) as
# llvm-objdump. Needs llvm-mc, llvm-objcopy and llvm-objdump with AMDGPU.
set -euo pipefail

mc=(llvm-mc --triple=amdgcn -mcpu=gfx1030 -mattr=+wavefrontsize32)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build -q -p rdna2-dis
status=0

# LLVM marks the form it chose (_e32, _e64, _dpp, _sdwa); AMD's names don't.
normalize() { sed -E 's/_(e32|e64|dpp|sdwa)$//'; }

for src in tests/asm/*.s; do
    name=$(basename "$src" .s)
    "${mc[@]}" -filetype=obj "$src" -o "$work/$name.o"
    llvm-objcopy -O binary --only-section=.text "$work/$name.o" "$work/$name.bin"
    llvm-objdump -d --mcpu=gfx1030 --mattr=+wavefrontsize32 "$work/$name.o" \
        | awk '/\/\/ [0-9A-Fa-f]+:/ { print $1 }' | normalize > "$work/$name.llvm"
    target/debug/rdna2-dis "$work/$name.bin" | awk '{ print $2 }' | normalize > "$work/$name.ours"
    if diff -u "$work/$name.llvm" "$work/$name.ours"; then
        echo "$name: $(wc -l < "$work/$name.llvm") instructions match"
    else
        status=1
    fi
done
exit $status
