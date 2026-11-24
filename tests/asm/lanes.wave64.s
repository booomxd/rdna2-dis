// Lane masks in 64-lane waves: VCC and SGPR pairs instead of single SGPRs.
v_cmp_eq_f32 vcc, v0, v1
v_cmp_eq_f32_e64 s[0:1], v0, v1
v_cndmask_b32 v0, v1, v2, vcc
v_cndmask_b32_e64 v0, v1, v2, s[4:5]
v_add_co_u32 v0, vcc, v1, v2
v_add_co_ci_u32 v0, vcc, v1, v2, vcc
s_and_saveexec_b64 s[0:1], vcc
s_endpgm
