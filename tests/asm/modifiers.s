// Modifiers and named operands beyond formats.s: DPP row controls, SDWA
// selects, DS offset pairs, VOP3 and VOP3P op_sel/neg, hwreg, sendmsg,
// depctr, clauses and buffer formats.
v_add_f32_dpp v0, v1, v2 row_shl:1 row_mask:0xf bank_mask:0xf
v_mov_b32_dpp v0, v1 row_mirror row_mask:0xf bank_mask:0xf
v_mov_b32_dpp v0, v1 row_share:3 row_mask:0xf bank_mask:0xf
v_mov_b32_dpp v0, v1 row_ror:4 row_mask:0x3 bank_mask:0xc
v_mov_b32_sdwa v0, v1 dst_sel:BYTE_1 dst_unused:UNUSED_PRESERVE src0_sel:BYTE_0
v_add_f32_sdwa v0, v1, v2 dst_sel:DWORD dst_unused:UNUSED_PAD src0_sel:WORD_1 src1_sel:BYTE_2
ds_write2_b32 v1, v2, v3 offset0:4 offset1:8
ds_read2_b32 v[0:1], v1 offset1:1
ds_read_b32 v0, v1 offset:65535
v_fma_f16 v0, v1, v2, v3 op_sel:[1,0,0,0]
v_pk_add_f16 v0, v1, v2 op_sel:[1,0] op_sel_hi:[0,1]
v_pk_fma_f16 v0, v1, v2, v3 neg_lo:[1,0,0] neg_hi:[0,0,1]
v_pk_mul_f16 v0, v1, v2 clamp
v_cmpx_eq_f32_e32 v0, v1
s_getreg_b32 s0, hwreg(HW_REG_TRAPSTS)
s_getreg_b32 s0, hwreg(HW_REG_HW_ID1)
s_getreg_b32 s0, hwreg(HW_REG_FLAT_SCR_LO)
s_setreg_b32 hwreg(HW_REG_MODE, 0, 4), s0
s_sendmsg sendmsg(MSG_INTERRUPT)
s_sendmsg sendmsg(MSG_GS, GS_OP_EMIT, 1)
s_sendmsg sendmsg(MSG_GS_DONE, GS_OP_NOP)
s_sendmsg sendmsg(MSG_SYSMSG, SYSMSG_OP_ECC_ERR_INTERRUPT)
s_sendmsg sendmsg(MSG_SYSMSG, SYSMSG_OP_TTRACE_PC)
s_waitcnt_depctr depctr_va_vdst(0)
s_waitcnt_depctr depctr_sa_sdst(0) depctr_vm_vsrc(0)
s_clause 0x3
tbuffer_load_format_xyzw v[0:3], v1, s[4:7], 0 format:[BUF_FMT_32_32_32_32_FLOAT] offen
tbuffer_load_format_x v0, v1, s[4:7], 0 format:[BUF_FMT_8_8_8_8_UNORM] idxen
tbuffer_store_format_x v0, v1, s[4:7], 0 format:[BUF_FMT_2_10_10_10_SNORM] offen
tbuffer_load_format_x v0, off, s[4:7], 0
global_load_dword v0, v[2:3], off offset:-8
global_load_dword v0, v1, s[2:3] offset:16 glc slc dlc
image_sample v[0:1], v[2:3], s[8:15], s[16:19] dmask:0x3 dim:SQ_RSRC_IMG_2D
s_endpgm
