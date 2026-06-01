#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  install-linux.sh --appdir <path-to-extracted-AppDir>

Installs an extracted Local Dictate AppDir for the current user under ~/.local.

This is the FUSE-free fallback after running:

  ./local-dictate-<version>-linux-x64.AppImage --appimage-extract
EOF
}

appdir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --appdir)
      appdir="${2:-}"
      shift 2
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

if [[ -z "$appdir" ]]; then
  usage
  exit 2
fi

install_root="${XDG_DATA_HOME:-$HOME/.local/share}/local-dictate"
bin_root="$HOME/.local/bin"
applications_root="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons_root="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
desktop_path="$applications_root/local-dictate.desktop"
icon_path="$icons_root/local-dictate.svg"
launcher_path="$bin_root/local-dictate"

mkdir -p "$install_root" "$bin_root" "$applications_root" "$icons_root"

appdir="$(realpath "$appdir")"
if [[ ! -d "$appdir" ]]; then
  echo "AppDir not found: $appdir" >&2
  exit 1
fi

if [[ ! -x "$appdir/AppRun" ]]; then
  echo "AppDir is missing executable AppRun: $appdir/AppRun" >&2
  exit 1
fi

rm -rf "$install_root/AppDir"
mkdir -p "$install_root/AppDir"
cp -a "$appdir"/. "$install_root/AppDir/"
chmod +x "$install_root/AppDir/AppRun"

cat > "$launcher_path" <<EOF
#!/usr/bin/env bash
exec "$install_root/AppDir/AppRun" "\$@"
EOF
chmod +x "$launcher_path"

desktop_source=""
icon_source=""

if [[ -n "$appdir" && -f "$appdir/local-dictate.desktop" ]]; then
  desktop_source="$appdir/local-dictate.desktop"
fi

if [[ -n "$appdir" && -f "$appdir/local-dictate.svg" ]]; then
  icon_source="$appdir/local-dictate.svg"
fi

if [[ -z "$desktop_source" && -f "$(dirname "$0")/../packaging/linux/local-dictate.desktop" ]]; then
  desktop_source="$(dirname "$0")/../packaging/linux/local-dictate.desktop"
fi

if [[ -z "$icon_source" && -f "$(dirname "$0")/../packaging/linux/local-dictate.svg" ]]; then
  icon_source="$(dirname "$0")/../packaging/linux/local-dictate.svg"
fi

if [[ -z "$desktop_source" ]]; then
  cat > "$desktop_path" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Local Dictate
GenericName=Dictation
Comment=Local-first push-to-talk dictation settings
Keywords=dictation;speech;voice;transcription;whisper;
Exec=$launcher_path
Icon=local-dictate
Terminal=false
Categories=Utility;Accessibility;
StartupNotify=true
StartupWMClass=local-dictate-ui
EOF
else
  install -m 0644 "$desktop_source" "$desktop_path"
  sed -i "s|^Exec=.*|Exec=$launcher_path|" "$desktop_path"
fi

if [[ -n "$icon_source" ]]; then
  install -m 0644 "$icon_source" "$icon_path"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$applications_root" >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Installed Local Dictate launcher: $desktop_path"
echo "Run it from your app launcher, or start it with: $launcher_path"
