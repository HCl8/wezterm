#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

VERSION=$(git log -1 --format='%cd-%h' --date=format:'%Y%m%d-%H%M%S')
ARCH="amd64"
PKG_NAME="wezterm-${VERSION}.Debian13"
PKG_DIR="/tmp/${PKG_NAME}"

echo "Building deb package: ${PKG_NAME}"

rm -rf "${PKG_DIR}"

# Binary files
install -Dm755 target/release/wezterm         "${PKG_DIR}/usr/bin/wezterm"
install -Dm755 target/release/wezterm-gui      "${PKG_DIR}/usr/bin/wezterm-gui"
install -Dm755 target/release/wezterm-mux-server "${PKG_DIR}/usr/bin/wezterm-mux-server"
install -Dm755 target/release/strip-ansi-escapes "${PKG_DIR}/usr/bin/strip-ansi-escapes"
install -Dm755 assets/open-wezterm-here        "${PKG_DIR}/usr/bin/open-wezterm-here"

# Shell integration
install -Dm644 assets/shell-integration/wezterm.sh "${PKG_DIR}/etc/profile.d/wezterm.sh"

# Shell completions
install -Dm644 assets/shell-completion/bash      "${PKG_DIR}/usr/share/bash-completion/completions/wezterm"
install -Dm644 assets/shell-completion/zsh       "${PKG_DIR}/usr/share/zsh/functions/Completion/Unix/_wezterm"

# Desktop file
install -Dm644 assets/wezterm.desktop           "${PKG_DIR}/usr/share/applications/org.wezfurlong.wezterm.desktop"

# Icon
install -Dm644 assets/icon/terminal.png         "${PKG_DIR}/usr/share/icons/hicolor/128x128/apps/org.wezfurlong.wezterm.png"

# Appdata
install -Dm644 assets/wezterm.appdata.xml       "${PKG_DIR}/usr/share/metainfo/org.wezfurlong.wezterm.appdata.xml"

# Nautilus extension
install -Dm644 assets/wezterm-nautilus.py        "${PKG_DIR}/usr/share/nautilus-python/extensions/wezterm-nautilus.py"

# KDE notifyrc
install -Dm644 assets/wezterm.notifyrc          "${PKG_DIR}/usr/share/knotifications6/wezterm.notifyrc"

# Empty share dir (matches reference)
install -dm755 "${PKG_DIR}/usr/share/wezterm"

# Strip binaries
strip "${PKG_DIR}/usr/bin/wezterm"
strip "${PKG_DIR}/usr/bin/wezterm-gui"
strip "${PKG_DIR}/usr/bin/wezterm-mux-server"
strip "${PKG_DIR}/usr/bin/strip-ansi-escapes"

# Compute installed size in KB
INSTALLED_SIZE=$(du -sk "${PKG_DIR}" | cut -f1)

# Determine dependencies
DEPENDS=$(dpkg-shlibdeps -O \
  "${PKG_DIR}/usr/bin/wezterm" \
  "${PKG_DIR}/usr/bin/wezterm-gui" \
  "${PKG_DIR}/usr/bin/wezterm-mux-server" \
  "${PKG_DIR}/usr/bin/strip-ansi-escapes" 2>/dev/null | sed 's/^shlibs:Depends=//' || true)

if [ -z "$DEPENDS" ]; then
  DEPENDS="libc6, libfontconfig1, libgcc-s1, libssl3, libwayland-client0, libwayland-egl1, libx11-6, libx11-xcb1, libxcb-image0, libxcb-util1, libxcb1, libxkbcommon-x11-0, libxkbcommon0, zlib1g"
fi

# Control file
mkdir -p "${PKG_DIR}/DEBIAN"
cat > "${PKG_DIR}/DEBIAN/control" <<EOF
Package: wezterm
Version: ${VERSION}
Conflicts: wezterm-nightly
Architecture: ${ARCH}
Maintainer: Wez Furlong <wez@wezfurlong.org>
Section: utils
Priority: optional
Homepage: https://wezfurlong.org/wezterm/
Installed-Size: ${INSTALLED_SIZE}
Provides: x-terminal-emulator
Depends: ${DEPENDS}
Description: Wez's Terminal Emulator.
 wezterm is a terminal emulator with support for modern features
 such as fonts with ligatures, hyperlinks, tabs and multiple
 windows.
EOF

# postinst
cat > "${PKG_DIR}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "configure" ] ; then
        update-alternatives --install /usr/bin/x-terminal-emulator x-terminal-emulator /usr/bin/open-wezterm-here 20
fi
EOF
chmod 755 "${PKG_DIR}/DEBIAN/postinst"

# prerm
cat > "${PKG_DIR}/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "remove" ]; then
	update-alternatives --remove x-terminal-emulator /usr/bin/open-wezterm-here
fi
EOF
chmod 755 "${PKG_DIR}/DEBIAN/prerm"

# Build the deb
DEB_FILE="${PKG_NAME}.deb"
dpkg-deb --build "${PKG_DIR}" "${DEB_FILE}"

echo ""
echo "=== Package built: ${DEB_FILE} ==="
ls -lh "${DEB_FILE}"
echo ""
echo "=== Contents: ==="
dpkg-deb -c "${DEB_FILE}" | head -40
echo ""
echo "=== Control: ==="
dpkg-deb -I "${DEB_FILE}"
