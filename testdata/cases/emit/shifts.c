uint32_t shifts(uint32_t a, uint32_t b) { RA_ENTER("shifts");
    return (((int32_t)((uint32_t)a << 3u) / (int32_t)b) + (((uint32_t)b >> 4u) % 10u)) | (uint32_t)((int32_t)a >> 2u);
}
