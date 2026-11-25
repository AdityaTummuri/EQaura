#!/usr/bin/env bash
# Builds .deb, .rpm and a portable .tar.xz from target/release/eqaura.
# Needs: dpkg-deb, rpmbuild. Usage: installer/linux/package.sh 0.1.0
set -euo pipefail
version="$1"
cd "$(dirname "$0")/../.."
root="$PWD/dist/linux/root"
rm -rf dist/linux && mkdir -p "$root"
install -Dm755 target/release/eqaura "$root/usr/bin/eqaura"
install -Dm644 installer/linux/eqaura.desktop "$root/usr/share/applications/eqaura.desktop"
install -Dm644 apps/desktop/assets/icon.svg "$root/usr/share/icons/hicolor/scalable/apps/eqaura.svg"
install -Dm644 apps/desktop/assets/icon-256.png "$root/usr/share/icons/hicolor/256x256/apps/eqaura.png"
install -Dm644 LICENSE "$root/usr/share/doc/eqaura/LICENSE"
install -Dm644 dist/THIRD-PARTY-LICENSES.txt "$root/usr/share/doc/eqaura/THIRD-PARTY-LICENSES.txt"

# Portable archive
XZ_OPT=-9 tar -C "$root/usr" -cJf "dist/EQaura-$version-linux-x86_64.tar.xz" bin share

# Debian and Ubuntu
deb="$PWD/dist/linux/deb"
cp -a "$root" "$deb"
mkdir -p "$deb/DEBIAN"
cat > "$deb/DEBIAN/control" <<EOF
Package: eqaura
Version: $version
Architecture: amd64
Maintainer: AdityaTummuri <AdityaTummuri@users.noreply.github.com>
Section: sound
Priority: optional
Depends: libc6, libasound2t64 | libasound2, libfontconfig1, libxkbcommon0, libxkbcommon-x11-0, libx11-6, libx11-xcb1, libxcursor1, libxi6, pipewire, pipewire-bin, wireplumber
Recommends: xdg-desktop-portal
Homepage: https://github.com/AdityaTummuri/EQaura
Description: System-wide equalizer
 Shape the sound of everything you play, with headphone correction,
 a hearing test and plain-English tuning.
EOF
dpkg-deb -Zxz -z9 --root-owner-group --build "$deb" "dist/EQaura-$version-linux-amd64.deb"

# Fedora and openSUSE
if command -v rpmbuild >/dev/null; then
  top="$PWD/dist/linux/rpm"
  mkdir -p "$top/SPECS"
  cat > "$top/SPECS/eqaura.spec" <<EOF
Name: eqaura
Version: $version
Release: 1
Summary: System-wide equalizer
License: MIT
URL: https://github.com/AdityaTummuri/EQaura
# Files and library names are the same on every distribution, package names are not.
Requires: pipewire, wireplumber, /usr/bin/pw-cli
Requires: libasound.so.2()(64bit), libfontconfig.so.1()(64bit)
Requires: libxkbcommon.so.0()(64bit), libxkbcommon-x11.so.0()(64bit)
Requires: libX11.so.6()(64bit), libX11-xcb.so.1()(64bit), libXcursor.so.1()(64bit), libXi.so.6()(64bit)
AutoReqProv: no
%description
Shape the sound of everything you play, with headphone correction, a hearing
test and plain-English tuning.
%install
cp -a $root/. %{buildroot}/
%files
/usr/bin/eqaura
/usr/share/applications/eqaura.desktop
/usr/share/icons/hicolor/scalable/apps/eqaura.svg
/usr/share/icons/hicolor/256x256/apps/eqaura.png
%doc /usr/share/doc/eqaura
EOF
  rpmbuild --define "_topdir $top" --define "_build_id_links none" --define "_binary_payload w9.xzdio" -bb "$top/SPECS/eqaura.spec"
  cp "$top"/RPMS/x86_64/*.rpm "dist/EQaura-$version-linux-x86_64.rpm"
fi
