# The PlayStation loader's fixture, built as the PSn00bSDK template builds its program: one GPREL
# executable, which psn00bsdk_add_executable links with the SDK's libraries and turns into a PS-EXE.
cmake_minimum_required(VERSION 3.21)

project(rhipper-psx-hello LANGUAGES C CXX ASM VERSION 1.0.0)

psn00bsdk_add_executable(hello GPREL hello.c)
