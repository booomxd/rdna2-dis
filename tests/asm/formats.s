// One or more instructions of every RDNA2 format, including the forms that
// add dwords: literals, DPP16, DPP8, SDWA and MIMG NSA.
s_mov_b32 s0, s1
s_mov_b32 s0, 0x12345678
s_add_u32 s0, s1, s2
s_add_u32 s0, s1, 0x12345678
s_movk_i32 s0, 0x1234
s_cmp_eq_u32 s0, s1
s_setreg_imm32_b32 hwreg(HW_REG_MODE), 0x0
s_load_dword s0, s[2:3], 0x0
v_mov_b32 v0, v1
v_mov_b32 v0, 0x3f800001
v_add_f32 v0, v1, v2
v_add_f32_e64 v0, v1, v2
v_cmp_eq_f32 vcc_lo, v0, v1
v_cndmask_b32 v0, v1, v2, vcc_lo
v_fmamk_f32 v0, v1, 0x40000000, v2
v_fmaak_f32 v0, v1, v2, 0x40000000
v_fma_f32 v0, v1, v2, v3
v_fma_f32 v0, v1, v2, 0x40000000
v_pk_add_f16 v0, v1, v2
v_mov_b32_dpp v0, v1 quad_perm:[1,0,3,2] row_mask:0xf bank_mask:0xf
v_mov_b32_dpp v0, v1 dpp8:[7,6,5,4,3,2,1,0]
v_mov_b32_sdwa v0, v1 dst_sel:WORD_1 dst_unused:UNUSED_PAD src0_sel:WORD_0
v_interp_p1_f32 v0, v1, attr0.x
ds_read_b32 v0, v1
buffer_load_dword v0, v1, s[4:7], 0 offen
tbuffer_load_format_x v0, v1, s[4:7], 0 format:[BUF_FMT_32_FLOAT] offen
image_sample v[0:3], v[4:5], s[8:15], s[16:19] dmask:0xf dim:SQ_RSRC_IMG_2D
image_sample v[0:3], [v4, v6], s[8:15], s[16:19] dmask:0xf dim:SQ_RSRC_IMG_2D
flat_load_dword v0, v[2:3]
global_load_dword v0, v[2:3], off
scratch_load_dword v0, v1, off
exp mrt0 v0, v1, v2, v3 done vm
s_endpgm
