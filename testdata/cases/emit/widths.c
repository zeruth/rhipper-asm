uint64_t widths(uint64_t x) { RA_ENTER("widths");
    return (uint64_t)(uint8_t)x + (uint64_t)(int16_t)(uint16_t)x;
}
