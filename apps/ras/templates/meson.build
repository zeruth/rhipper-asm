# The Meson project of the port of @NAME@. ras wrote this file because the project had none.
# ras does not write it again, so you can change it.
project('@TARGET@', 'c', 'cpp', default_options: ['c_std=c11', 'cpp_std=c++17', 'warning_level=1'])

fs = import('fs')
cc = meson.get_compiler('c')

# The translation that ras emit writes, with its runtime beside it, and the hand-written sources
# of the port (`sources` in rhipper.json) but those under the translation: the functions
# rhipper.json names under `hand` are theirs to define, ras emit only declares them. Meson does
# not search for sources, so the pipeline's build step lists them in @SOURCE_LIST@.
gen_dir = '@GEN_DIR@'
sources = files(fs.read('@SOURCE_LIST@').strip().split('\n'))
includes = include_directories(gen_dir, gen_dir / 'runtime')

# The translation uses the clang extensions the runtime header relies on: 128-bit integers and
# vector types, so the compiler is clang (CC=clang, CXX=clang++).
args = ['-O1', '-Wno-unused-variable', '-Wno-unused-but-set-variable', '-Wno-parentheses']
link_args = []
deps = []
if host_machine.system() == 'windows'
  link_args += ['-fuse-ld=lld', '-lkernel32', '-luser32']
  # A 32-bit program keeps its 32-bit pointers in the port: every address of the process must
  # fit in them, which the linker arranges for a binary that is not large-address-aware.
  if @POINTER_BITS@ == 32
    link_args += '-Wl,/largeaddressaware:no'
  endif
  # The program's resources, which ras emit extracts from the image: linked into the port, so
  # the program's FindResource and LoadString find them as they did.
  if fs.exists(gen_dir / 'resources.res')
    link_args += meson.current_source_dir() / gen_dir / 'resources.res'
  endif
else
  deps += [cc.find_library('dl', required: false), cc.find_library('m', required: false)]
endif

# A program is an executable; a DLL is a shared library whose exports (by name and ordinal, with
# the forwarders) are the image's, from the definition file ras emit writes beside the
# translation.
if @DLL@
  shared_library('@TARGET@', sources, include_directories: includes, c_args: args, cpp_args: args, link_args: link_args,
    dependencies: deps, vs_module_defs: gen_dir / 'exports.def', name_prefix: '')
else
  executable('@TARGET@', sources, include_directories: includes, c_args: args, cpp_args: args, link_args: link_args,
    dependencies: deps)
endif
