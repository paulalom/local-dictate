#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage: package-linux-appimage.sh \
  --version <version> \
  --binary-dir <dir> \
  --output-dir <dir> \
  --linuxdeploy <path> \
  --appimagetool <path> \
  [--default-model base.en]
EOF
}

version=""
binary_dir=""
output_dir="dist"
linuxdeploy=""
appimagetool=""
default_model="base.en"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      version="${2:-}"
      shift 2
      ;;
    --binary-dir)
      binary_dir="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    --linuxdeploy)
      linuxdeploy="${2:-}"
      shift 2
      ;;
    --appimagetool)
      appimagetool="${2:-}"
      shift 2
      ;;
    --default-model)
      default_model="${2:-}"
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

if [[ -z "$version" || -z "$binary_dir" || -z "$linuxdeploy" || -z "$appimagetool" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary_root="$(realpath "$binary_dir")"
output_root="$(mkdir -p "$output_dir" && realpath "$output_dir")"
stage_root="$repo_root/.local/appimage"
appdir="$stage_root/Local_Dictate.AppDir"
model_file="ggml-$default_model.bin"
appimage_path="$output_root/local-dictate-$version-linux-x64.AppImage"

require_file() {
  local path="$1"

  if [[ ! -f "$path" ]]; then
    echo "Missing required file: $path" >&2
    exit 1
  fi
}

require_file "$binary_root/local-dictate-ui"
require_file "$binary_root/local-dictate-cli"
require_file "$repo_root/engines/whisper-cli"
require_file "$repo_root/models/$model_file"
require_file "$repo_root/packaging/linux/local-dictate.desktop"
require_file "$repo_root/packaging/linux/local-dictate.svg"
require_file "$linuxdeploy"
require_file "$appimagetool"

export APPIMAGE_EXTRACT_AND_RUN=1

rm -rf "$appdir"
mkdir -p \
  "$appdir/usr/bin" \
  "$appdir/usr/lib/local-dictate/engines" \
  "$appdir/usr/lib/local-dictate/models" \
  "$appdir/usr/share/applications" \
  "$appdir/usr/share/icons/hicolor/scalable/apps"

install -m 0755 "$binary_root/local-dictate-ui" "$appdir/usr/bin/local-dictate-ui"
install -m 0755 "$binary_root/local-dictate-cli" "$appdir/usr/bin/local-dictate-cli"
install -m 0755 "$repo_root/engines/whisper-cli" "$appdir/usr/lib/local-dictate/engines/whisper-cli"
install -m 0644 "$repo_root/models/$model_file" "$appdir/usr/lib/local-dictate/models/$model_file"
install -m 0644 "$repo_root/packaging/linux/local-dictate.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/local-dictate.svg"
install -m 0644 "$repo_root/packaging/linux/local-dictate.svg" "$appdir/local-dictate.svg"
install -m 0644 "$repo_root/packaging/linux/local-dictate.desktop" "$appdir/local-dictate.desktop"
install -m 0644 "$repo_root/packaging/linux/local-dictate.desktop" "$appdir/usr/share/applications/local-dictate.desktop"

"$linuxdeploy" \
  --appdir "$appdir" \
  --desktop-file "$appdir/local-dictate.desktop" \
  --icon-file "$appdir/local-dictate.svg" \
  --executable "$appdir/usr/bin/local-dictate-ui" \
  --executable "$appdir/usr/bin/local-dictate-cli" \
  --executable "$appdir/usr/lib/local-dictate/engines/whisper-cli"

rm -f "$appdir/AppRun"
cat > "$appdir/AppRun" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

appdir="$(dirname "$(readlink -f "$0")")"
export PATH="$appdir/usr/bin:$PATH"
export LOCAL_DICTATE_ASSET_DIR="$appdir/usr/lib/local-dictate"

exec "$appdir/usr/bin/local-dictate-ui" "$@"
EOF
chmod +x "$appdir/AppRun"

rm -f "$appimage_path"
ARCH=x86_64 "$appimagetool" "$appdir" "$appimage_path"
chmod +x "$appimage_path"

echo "Packaged $appimage_path"
