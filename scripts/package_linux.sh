#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-0.1.0}"
CLEAN_VERSION="${VERSION#v}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_DIR="${REPO_ROOT}/app/build/linux/x64/release/bundle"
DIST_DIR="${REPO_ROOT}/dist"

if [[ ! -x "${BUNDLE_DIR}/tool_center" ]]; then
  echo "Error: Linux release bundle not found at ${BUNDLE_DIR}. Run 'flutter build linux --release' first." >&2
  exit 1
fi

mkdir -p "${DIST_DIR}"

# Generate clean vector icon used by both .deb and AppImage
ICON_SVG="${DIST_DIR}/io.toolcenter.tool_center.svg"
cat > "${ICON_SVG}" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <rect width="256" height="256" rx="56" fill="#2563EB"/>
  <path d="M76 88h104v22H76zm0 38h72v22H76zm0 38h104v22H76z" fill="#FFFFFF"/>
  <circle cx="174" cy="137" r="14" fill="#93C5FD"/>
</svg>
EOF

DESKTOP_CONTENT="[Desktop Entry]
Type=Application
Name=Tool Center
GenericName=Desktop File Toolbox
Comment=Fast, offline file preview, conversion, and manipulation toolbox
Exec=tool_center %f
Icon=io.toolcenter.tool_center
Terminal=false
Categories=Utility;FileTools;AudioVideo;Graphics;
StartupWMClass=io.toolcenter.tool_center
"

# ---------------------------------------------------------------------------
# 1. Build Debian (.deb) package
# ---------------------------------------------------------------------------
DEB_STAGE="$(mktemp -d)"
trap 'rm -rf "${DEB_STAGE}"' EXIT

mkdir -p \
  "${DEB_STAGE}/DEBIAN" \
  "${DEB_STAGE}/opt/tool-center" \
  "${DEB_STAGE}/usr/bin" \
  "${DEB_STAGE}/usr/share/applications" \
  "${DEB_STAGE}/usr/share/icons/hicolor/scalable/apps"

cp -a "${BUNDLE_DIR}/." "${DEB_STAGE}/opt/tool-center/"
ln -sf /opt/tool-center/tool_center "${DEB_STAGE}/usr/bin/tool_center"
printf '%s' "${DESKTOP_CONTENT}" > "${DEB_STAGE}/usr/share/applications/io.toolcenter.tool_center.desktop"
cp "${ICON_SVG}" "${DEB_STAGE}/usr/share/icons/hicolor/scalable/apps/io.toolcenter.tool_center.svg"

INSTALLED_KB="$(du -sk "${DEB_STAGE}/opt" "${DEB_STAGE}/usr" | awk '{s+=$1} END {print s}')"

cat > "${DEB_STAGE}/DEBIAN/control" <<EOF
Package: tool-center
Version: ${CLEAN_VERSION}
Section: utils
Priority: optional
Architecture: amd64
Installed-Size: ${INSTALLED_KB}
Depends: libgtk-3-0, libmpv2 | libmpv1
Maintainer: Tool Center Contributors <opensource@toolcenter.io>
Homepage: https://github.com/musamusakannike/tool-center
Description: Fast, lightweight, offline desktop file preview and conversion toolbox
 Tool Center previews images, video, audio, PDFs, archives, and structured text
 files while providing 35+ offline file conversion and manipulation tools.
EOF

DEB_OUT="${DIST_DIR}/tool-center_${CLEAN_VERSION}_amd64.deb"
dpkg-deb --build --root-owner-group "${DEB_STAGE}" "${DEB_OUT}"
echo "Built Debian package: ${DEB_OUT}"

# ---------------------------------------------------------------------------
# 2. Build AppImage (if --skip-appimage is not passed)
# ---------------------------------------------------------------------------
if [[ "${2:-}" != "--skip-appimage" ]]; then
  APPDIR="$(mktemp -d)"
  mkdir -p "${APPDIR}/usr/bin" "${APPDIR}/usr/share/icons/hicolor/scalable/apps"
  cp -a "${BUNDLE_DIR}/." "${APPDIR}/usr/bin/"

  cat > "${APPDIR}/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
export LD_LIBRARY_PATH="${HERE}/usr/bin/lib:${HERE}/usr/bin/data/ffmpeg/lib:${LD_LIBRARY_PATH:-}"
exec "${HERE}/usr/bin/tool_center" "$@"
EOF
  chmod +x "${APPDIR}/AppRun"

  printf '%s' "${DESKTOP_CONTENT}" > "${APPDIR}/io.toolcenter.tool_center.desktop"
  cp "${ICON_SVG}" "${APPDIR}/io.toolcenter.tool_center.svg"
  cp "${ICON_SVG}" "${APPDIR}/.DirIcon"

  APPIMAGETOOL="${DIST_DIR}/appimagetool-x86_64.AppImage"
  if [[ ! -x "${APPIMAGETOOL}" ]]; then
    curl -fsSL -o "${APPIMAGETOOL}" \
      "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x "${APPIMAGETOOL}"
  fi

  APPIMAGE_OUT="${DIST_DIR}/Tool-Center-${CLEAN_VERSION}-linux-x86_64.AppImage"
  ARCH=x86_64 "${APPIMAGETOOL}" --appimage-extract-and-run "${APPDIR}" "${APPIMAGE_OUT}"
  chmod +x "${APPIMAGE_OUT}"
  rm -rf "${APPDIR}"
  echo "Built AppImage: ${APPIMAGE_OUT}"
fi

rm -f "${ICON_SVG}"
