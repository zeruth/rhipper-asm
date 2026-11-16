uint32_t shared_tail(uint32_t n, uint8_t * p) { RA_ENTER("shared_tail");
    uint32_t i;
    if (n != 0u) {
        for (;;) {
            i = *(uint32_t *)(p);
            if ((int32_t)i < (int32_t)n) {
                if (i == 7u) {
                    *(uint32_t *)(p) = 1u;
                } else {
                    *(uint32_t *)(p) = i + 1u;
                    continue;
                }
            } else {
                *(uint32_t *)(p) = 0u;
            }
            goto L_ret;
        }
    }
L_ret:;
    *(uint32_t *)(p) = 1u;
    *(uint32_t *)(p) = 2u;
    *(uint32_t *)(p) = 3u;
    *(uint32_t *)(p) = 4u;
    *(uint32_t *)(p) = 5u;
    *(uint32_t *)(p) = 6u;
    *(uint32_t *)(p) = 7u;
    *(uint32_t *)(p) = 8u;
    *(uint32_t *)(p) = 9u;
    *(uint32_t *)(p) = 10u;
    *(uint32_t *)(p) = 11u;
    *(uint32_t *)(p) = 12u;
    *(uint32_t *)(p) = 13u;
    *(uint32_t *)(p) = 14u;
    *(uint32_t *)(p) = 15u;
    *(uint32_t *)(p) = 16u;
    return *(uint32_t *)(p);
}
