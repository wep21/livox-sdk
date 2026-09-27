#!/usr/bin/env bash
# Usage: tests/upstream/regenerate.sh /path/Livox-SDK /path/Livox-SDK2
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
sdk1="$(cd "$1" && pwd)"
sdk2="$(cd "$2" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cxx="${CXX:-c++}"
"$cxx" -std=c++11 -I"$sdk2/sdk_core" -I"$sdk2/include" -I"$sdk2/3rdparty" "$root/tests/upstream/sdk2_reference.cpp" "$sdk2/sdk_core/comm/sdk_protocol.cpp" "$sdk2/3rdparty/FastCRC/FastCRCsw.cpp" -o "$tmp/sdk2-ref"
"$cxx" -std=c++11 -I"$sdk1/sdk_core/include" -I"$sdk1/sample/lidar_lvx_file" "$root/tests/upstream/sdk1_reference.cpp" "$sdk1/sample/lidar_lvx_file/lvx_file.cpp" -o "$tmp/sdk1-ref"
(cd "$tmp"; ./sdk2-ref; ./sdk1-ref)
cp "$tmp"/*.bin "$root/fixtures/"
cp "$tmp"/*.lvx "$root/fixtures/sdk1-writer.lvx"
