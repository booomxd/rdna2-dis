// Labels and branches, forward and backward: labels become symbols in the
// object, and both disassemblers must name them in the same places.
main:
s_mov_b32 s0, 0
loop:
s_add_u32 s0, s0, 1
s_cmp_lt_u32 s0, 8
s_cbranch_scc1 loop
s_cbranch_execz done
v_mov_b32 v0, s0
s_branch done
s_nop 0
done:
s_endpgm
