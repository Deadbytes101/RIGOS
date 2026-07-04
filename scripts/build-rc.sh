#!/usr/bin/env bash
set -euo pipefail
die() { printf 'build-rc: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 1 ]] || die "usage: $0 v0.0.1-rcN"
rc="$1"
[[ "$rc" =~ ^v0\.0\.1-rc[1-9][0-9]*$ ]] || die "invalid RC identifier"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"; cd "$root"
[[ -f Cargo.lock ]] || die "Cargo.lock is missing"
git diff --quiet || die "working tree is dirty"
git diff --cached --quiet || die "index is dirty"
[[ -z "$(git status --porcelain)" ]] || die "untracked files exist"
commit="$(git rev-parse --verify HEAD)"
./scripts/verify.sh
export RIGOS_BUILD_COMMIT="$commit" RUSTFLAGS="-C target-cpu=x86-64" SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
cargo build --release --locked --target x86_64-unknown-linux-gnu
binary="target/x86_64-unknown-linux-gnu/release/rigosd"
[[ -x "$binary" ]] || die "release binary was not produced"
file "$binary" | grep -q 'ELF 64-bit LSB.*x86-64' || die "unexpected ELF architecture"
readelf -l "$binary" | grep -q '/lib64/ld-linux-x86-64.so.2' || die "unexpected ELF interpreter"
readelf -d "$binary" | grep -E 'NEEDED.*(libcuda|libamdhip64|libstdc\+\+)' >/dev/null && die "unexpected runtime dependency"
out="dist/$rc"; rm -rf -- "$out"; mkdir -p "$out/schemas" "$out/docs"
install -m 0755 "$binary" "$out/rigosd"; ln -s rigosd "$out/rigosctl"; cp schemas/*.json "$out/schemas/"
cp docs/physical-rig-validation.md docs/threat-model.md "$out/docs/"
build_os="$(. /etc/os-release; printf '%s %s' "$NAME" "$VERSION_ID")"
cargo run --quiet --locked -p rigos-evidence -- build-manifest --rc "$rc" --commit "$commit" \
  --binary "$out/rigosd" --schemas "$out/schemas" --output "$out/BUILD-MANIFEST.json" \
  --rustc "$(rustc --version --verbose | tr '\n' ';')" --cargo "$(cargo --version)" \
  --build-os "$build_os" --kernel "$(uname -srmo)" >/dev/null
file "$out/rigosd" > "$out/ELF-REPORT.txt"; readelf -h "$out/rigosd" >> "$out/ELF-REPORT.txt"
readelf -l "$out/rigosd" >> "$out/ELF-REPORT.txt"; readelf -d "$out/rigosd" >> "$out/ELF-REPORT.txt"; ldd "$out/rigosd" >> "$out/ELF-REPORT.txt"
(cd "$out" && find . \( -type f -o -type l \) ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
printf 'Authoritative RC created: %s\nCommit: %s\n' "$out" "$commit"
