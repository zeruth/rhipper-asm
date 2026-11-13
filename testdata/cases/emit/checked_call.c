// 0x1000
uint64_t F_00001000(uint64_t rbx) {
    RA_ENTER("F_00001000");
    uint64_t rbx_after;
    rbx_after = F_00002000(rbx);
    ra_assume(0, rbx == rbx_after);
    return rbx_after;
}
