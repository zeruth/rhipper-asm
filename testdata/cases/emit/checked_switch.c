// 0x1100
uint64_t F_00001100(uint64_t index) {
    RA_ENTER("F_00001100");
    ra_assume(1, index < 2ull);
    switch (index) {
    case 0:
        return 10ull;
    case 1:
        return 20ull;
    default:
        ra_trap(4360ull, 7ull);
        RA_UNREACHABLE();
    }
}
