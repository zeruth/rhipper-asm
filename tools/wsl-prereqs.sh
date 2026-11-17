#!/bin/bash
# Checks or installs the WSL (Ubuntu 24.04) packages the foreign-architecture cards need: the
# emulators the vector harness and the fixtures' expected outputs run under, clang and lld with
# every target (the cross compilers of testdata/build.ps1 and the harness), and csmith with
# creduce for the generated programs. Run it by path so no shell has to split a quoted script:
#
#   wsl -d Ubuntu-24.04 -- /mnt/c/.../tools/wsl-prereqs.sh --check
#   wsl -d Ubuntu-24.04 -- /mnt/c/.../tools/wsl-prereqs.sh --install
#
# --check prints `<package> ok` or `<package> missing` per package and exits 1 when one is
# missing; --install installs the missing ones with passwordless sudo, then checks again.
set -u

packages=(
  qemu-user-static
  clang
  lld
  llvm
  csmith
  libcsmith-dev
  creduce
)

usage() {
  echo "usage: wsl-prereqs.sh --check | --install" >&2
  exit 2
}

installed() {
  [ "$(dpkg-query -W -f='${Status}' "$1" 2>/dev/null)" = "install ok installed" ]
}

# Prints one line per package and the missing ones into the global `missing`.
check() {
  missing=()
  for package in "${packages[@]}"; do
    if installed "$package"; then
      echo "$package ok"
    else
      echo "$package missing"
      missing+=("$package")
    fi
  done
}

[ $# -eq 1 ] || usage
case "$1" in
  --check)
    check
    [ ${#missing[@]} -eq 0 ] || exit 1
    ;;
  --install)
    check
    if [ ${#missing[@]} -gt 0 ]; then
      if ! sudo -n true 2>/dev/null; then
        echo "wsl-prereqs.sh: sudo asks for a password; install by hand: sudo apt-get install -y ${missing[*]}" >&2
        exit 1
      fi
      sudo -n apt-get update -q || exit 1
      sudo -n DEBIAN_FRONTEND=noninteractive apt-get install -y -q "${missing[@]}" || exit 1
      check
      [ ${#missing[@]} -eq 0 ] || exit 1
    fi
    ;;
  *)
    usage
    ;;
esac
