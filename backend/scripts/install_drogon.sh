#!/usr/bin/env bash
set -euo pipefail

# Ubuntu 24.04's packaged Drogon predates async response streams. Keep CI and
# production builds on the same upstream version because Otto streams SSE.
drogon_version="${DROGON_VERSION:-v1.9.13}"
install_prefix="${DROGON_INSTALL_PREFIX:-/usr/local}"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

git clone --quiet --depth 1 --branch "$drogon_version" --recurse-submodules \
  https://github.com/drogonframework/drogon.git "$work_dir/drogon"

cmake -S "$work_dir/drogon" -B "$work_dir/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$install_prefix" \
  -DBUILD_SHARED_LIBS=OFF \
  -DBUILD_CTL=OFF \
  -DBUILD_EXAMPLES=OFF \
  -DBUILD_ORM=OFF \
  -DBUILD_BROTLI=OFF \
  -DBUILD_YAML_CONFIG=OFF \
  -DUSE_SUBMODULE=ON
cmake --build "$work_dir/build" --parallel 2
cmake --install "$work_dir/build"
