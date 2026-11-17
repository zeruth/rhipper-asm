uint32_t shared_tail(uint32_t n, uint8_t * p) { RA_ENTER("shared_tail");
    uint32_t i;
    if (n != 0u) {
        for (;;) {
            i = RA_U32(p);
            if ((int32_t)i < (int32_t)n) {
                if (i == 7u) {
                    RA_U32(p) = 1u;
                } else {
                    RA_U32(p) = i + 1u;
                    continue;
                }
            } else {
                RA_U32(p) = 0u;
            }
            goto L_ret;
        }
    }
L_ret:;
    RA_U32(p) = 1u;
    RA_U32(p) = 2u;
    RA_U32(p) = 3u;
    RA_U32(p) = 4u;
    RA_U32(p) = 5u;
    RA_U32(p) = 6u;
    RA_U32(p) = 7u;
    RA_U32(p) = 8u;
    RA_U32(p) = 9u;
    RA_U32(p) = 10u;
    RA_U32(p) = 11u;
    RA_U32(p) = 12u;
    RA_U32(p) = 13u;
    RA_U32(p) = 14u;
    RA_U32(p) = 15u;
    RA_U32(p) = 16u;
    return RA_U32(p);
}
