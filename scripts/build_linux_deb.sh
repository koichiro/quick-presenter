#!/usr/bin/env bash
set -euo pipefail

artifact_dir="artifacts/quick-presenter-ubuntu-x64"
binary="target/release/quick-presenter"
output_deb=""
keep_work_dir=0

usage() {
  cat >&2 <<'EOF'
Usage: scripts/build_linux_deb.sh [--artifact-dir DIR] [--binary PATH] [--output-deb PATH] [--keep-work-dir]
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-dir)
      artifact_dir="${2:-}"
      shift 2
      ;;
    --binary)
      binary="${2:-}"
      shift 2
      ;;
    --output-deb)
      output_deb="${2:-}"
      shift 2
      ;;
    --keep-work-dir)
      keep_work_dir=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$artifact_dir" || -z "$binary" ]]; then
  usage
  exit 2
fi

version="$(
  awk -F '"' '/^version = / { print $2; exit }' Cargo.toml
)"

if [[ -z "$version" ]]; then
  echo "Could not read package version from Cargo.toml" >&2
  exit 1
fi

case "$(uname -m)" in
  x86_64|amd64)
    deb_arch="amd64"
    ;;
  aarch64|arm64)
    deb_arch="arm64"
    ;;
  *)
    echo "Unsupported Debian package architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "Missing dpkg-deb; install dpkg tooling before building the Debian package" >&2
  exit 1
fi

if [[ ! -x "$binary" ]]; then
  echo "Missing executable binary: $binary" >&2
  exit 1
fi

if [[ ! -d "pdfium" ]]; then
  echo "Missing bundled PDFium directory: pdfium" >&2
  exit 1
fi

if [[ ! -s "pdfium/LICENSE" ]]; then
  echo "Missing PDFium license file: pdfium/LICENSE" >&2
  exit 1
fi

if [[ ! -s "pdfium/VERSION" ]]; then
  echo "Missing PDFium version file: pdfium/VERSION" >&2
  exit 1
fi

if [[ ! -d "pdfium/licenses" ]]; then
  echo "Missing PDFium component licenses directory: pdfium/licenses" >&2
  exit 1
fi

if [[ ! -s "LICENSE" ]]; then
  echo "Missing Quick Presenter license file: LICENSE" >&2
  exit 1
fi

if [[ ! -s "packaging/SOURCE-OFFER.txt" ]]; then
  echo "Missing source offer file: packaging/SOURCE-OFFER.txt" >&2
  exit 1
fi

mkdir -p "$artifact_dir"

package_root="$artifact_dir/linux-deb-root"
debian_dir="$package_root/DEBIAN"
install_dir="$package_root/usr/lib/quick-presenter"
bin_dir="$package_root/usr/bin"
runtime_license_dir="$install_dir/licenses"
doc_dir="$package_root/usr/share/doc/quick-presenter"

if [[ -z "$output_deb" ]]; then
  output_deb="$artifact_dir/quick-presenter_${version}_${deb_arch}.deb"
fi

normalize_deb_package_modes() {
  find "$package_root" -type d -exec chmod 755 {} +
  find "$package_root" -type f -exec chmod 644 {} +
  chmod 755 "$install_dir/quick-presenter"
}

rm -rf "$package_root"
mkdir -p "$debian_dir" "$install_dir" "$bin_dir" "$runtime_license_dir" "$doc_dir"

cp "$binary" "$install_dir/quick-presenter"
chmod 755 "$install_dir/quick-presenter"
ln -s "../lib/quick-presenter/quick-presenter" "$bin_dir/quick-presenter"

cp -R "pdfium" "$install_dir/pdfium"
cp "LICENSE" "$runtime_license_dir/QuickPresenter-LICENSE.txt"
cp "packaging/SOURCE-OFFER.txt" "$runtime_license_dir/QuickPresenter-SOURCE-OFFER.txt"
cp "pdfium/LICENSE" "$runtime_license_dir/PDFium-LICENSE.txt"

cp "LICENSE" "$doc_dir/QuickPresenter-LICENSE.txt"
cp "packaging/SOURCE-OFFER.txt" "$doc_dir/QuickPresenter-SOURCE-OFFER.txt"
cp "pdfium/LICENSE" "$doc_dir/PDFium-LICENSE.txt"

scripts/stage_linux_desktop_assets.sh "$package_root/usr"

normalize_deb_package_modes

installed_size="$(du -ks "$package_root/usr" | awk '{ print $1 }')"

cat > "$debian_dir/control" <<EOF
Package: quick-presenter
Version: $version
Section: office
Priority: optional
Architecture: $deb_arch
Maintainer: Quick Presenter Maintainers <koichiro@users.noreply.github.com>
Installed-Size: $installed_size
Depends: libc6, libgcc-s1, libstdc++6, libfontconfig1, libx11-6, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-cursor0, libwayland-egl1, libegl1, libgl1, libgtk-3-0
Homepage: https://github.com/koichiro/quick-presenter
Description: Presenter-focused PDF slide deck playback
 Quick Presenter is a lightweight presenter tool for prepared PDF slide decks.
 It focuses on reliable playback, presenter controls, and bundled PDFium
 runtime files for predictable startup without first-launch downloads.
EOF

dpkg-deb --build --root-owner-group "$package_root" "$output_deb"

if [[ ! -s "$output_deb" ]]; then
  echo "Debian package was not created: $output_deb" >&2
  exit 1
fi

if [[ "$keep_work_dir" -ne 1 ]]; then
  rm -rf "$package_root"
fi

echo "Created $output_deb"
