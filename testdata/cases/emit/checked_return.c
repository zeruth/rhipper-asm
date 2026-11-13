// 0x1200
uint64_t F_00001200(uint64_t rsp, uint64_t n) {
    RA_ENTER("F_00001200");
    uint64_t rsp_end;
    rsp_end = (rsp - n) + n;
    ra_assume(2, rsp_end == rsp);
    return n;
}
