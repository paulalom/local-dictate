#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  install-linux.sh
  install-linux.sh --appimage <path-to-AppImage>
  install-linux.sh --appdir <path-to-extracted-AppDir>

Installs Local Dictate for the current user under ~/.local.

Run without arguments when the installer is next to its matching AppImage. Use
--appimage to target a specific AppImage manually. The installer copies the
AppImage into ~/.local/share/local-dictate and creates an app launcher entry.

Use --appdir as the FUSE-free fallback after running:

  ./local-dictate-<version>-linux-x64.AppImage --appimage-extract
EOF
}

appimage=""
appdir=""
script_path="$(realpath "$0")"
script_dir="$(dirname "$script_path")"
script_name="$(basename "$script_path")"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --appimage)
      appimage="${2:-}"
      shift 2
      ;;
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

if [[ -n "$appimage" && -n "$appdir" ]]; then
  echo "Use either --appimage or --appdir, not both." >&2
  usage
  exit 2
fi

if [[ -z "$appimage" && -z "$appdir" ]]; then
  if [[ "$script_name" == *.install.sh ]]; then
    inferred_appimage="$script_dir/${script_name%.install.sh}.AppImage"
    if [[ -f "$inferred_appimage" ]]; then
      appimage="$inferred_appimage"
    fi
  fi

  if [[ -z "$appimage" ]]; then
    shopt -s nullglob
    candidates=("$script_dir"/local-dictate-*-linux-x64.AppImage)
    shopt -u nullglob

    if [[ "${#candidates[@]}" -eq 1 ]]; then
      appimage="${candidates[0]}"
    elif [[ "${#candidates[@]}" -gt 1 ]]; then
      echo "Multiple Local Dictate AppImages found next to installer; pass --appimage explicitly." >&2
      printf '  %s\n' "${candidates[@]}" >&2
      exit 2
    fi
  fi

  if [[ -z "$appimage" ]]; then
    echo "Could not find matching AppImage next to installer: $script_dir" >&2
    usage
    exit 2
  fi
fi

install_root="${XDG_DATA_HOME:-$HOME/.local/share}/local-dictate"
bin_root="$HOME/.local/bin"
applications_root="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons_root="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
metainfo_root="${XDG_DATA_HOME:-$HOME/.local/share}/metainfo"
desktop_path="$applications_root/local-dictate.desktop"
icon_path="$icons_root/local-dictate.svg"
metainfo_path="$metainfo_root/local-dictate.appdata.xml"
launcher_path="$bin_root/local-dictate"
installed_appimage="$install_root/LocalDictate.AppImage"

mkdir -p "$install_root" "$bin_root" "$applications_root" "$icons_root" "$metainfo_root"

desktop_source=""
icon_source=""
metainfo_source=""

running_install_mountpoints() {
  if ! command -v findmnt >/dev/null 2>&1; then
    return 0
  fi

  findmnt -rn -o TARGET,SOURCE |
    awk '$2 == "LocalDictate.AppImage" { print $1 }'
}

running_install_pids() {
  local patterns=(
    "$installed_appimage"
    "$install_root/AppDir/AppRun"
    "$install_root/AppDir/usr/bin/local-dictate-ui"
  )

  local mountpoint
  while IFS= read -r mountpoint; do
    patterns+=("$mountpoint/")
  done < <(running_install_mountpoints)

  ps -eo pid=,args= |
    awk -v pattern_list="$(printf '%s\n' "${patterns[@]}")" '
      BEGIN {
        count = split(pattern_list, patterns, "\n")
      }
      {
        pid = $1
        line = $0
        sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", line)
        for (i = 1; i <= count; i++) {
          if (patterns[i] != "" && index(line, patterns[i]) > 0) {
            print pid
            break
          }
        }
      }
    ' |
    sort -u
}

stop_running_install() {
  local pids=()
  mapfile -t pids < <(running_install_pids)

  if [[ "${#pids[@]}" -eq 0 ]]; then
    return 0
  fi

  echo "Stopping running Local Dictate instance before updating..."
  kill -TERM "${pids[@]}" 2>/dev/null || true

  for _ in {1..30}; do
    sleep 0.1
    mapfile -t pids < <(running_install_pids)
    if [[ "${#pids[@]}" -eq 0 ]]; then
      return 0
    fi
  done

  kill -KILL "${pids[@]}" 2>/dev/null || true
}

if [[ -n "$appimage" ]]; then
  appimage="$(realpath "$appimage")"
  if [[ ! -f "$appimage" ]]; then
    echo "AppImage not found: $appimage" >&2
    exit 1
  fi

  stop_running_install

  tmp_appimage="$(mktemp "$install_root/LocalDictate.AppImage.XXXXXX")"
  rm -rf "$install_root/AppDir"
  install -m 0755 "$appimage" "$tmp_appimage"
  mv -f "$tmp_appimage" "$installed_appimage"

  extract_root="$(mktemp -d)"
  trap 'rm -rf "$extract_root"' EXIT
  (
    cd "$extract_root"
    "$installed_appimage" --appimage-extract local-dictate.desktop >/dev/null
    "$installed_appimage" --appimage-extract usr/share/icons/hicolor/scalable/apps/local-dictate.svg >/dev/null
    "$installed_appimage" --appimage-extract usr/share/metainfo/local-dictate.appdata.xml >/dev/null
  ) || true

  if [[ -f "$extract_root/squashfs-root/local-dictate.desktop" ]]; then
    desktop_source="$extract_root/squashfs-root/local-dictate.desktop"
  fi

  if [[ -f "$extract_root/squashfs-root/usr/share/icons/hicolor/scalable/apps/local-dictate.svg" ]]; then
    icon_source="$extract_root/squashfs-root/usr/share/icons/hicolor/scalable/apps/local-dictate.svg"
  fi

  if [[ -f "$extract_root/squashfs-root/usr/share/metainfo/local-dictate.appdata.xml" ]]; then
    metainfo_source="$extract_root/squashfs-root/usr/share/metainfo/local-dictate.appdata.xml"
  fi

  cat > "$launcher_path" <<EOF
#!/usr/bin/env bash
exec "$installed_appimage" "\$@"
EOF
else
  appdir="$(realpath "$appdir")"
  if [[ ! -d "$appdir" ]]; then
    echo "AppDir not found: $appdir" >&2
    exit 1
  fi

  if [[ ! -x "$appdir/AppRun" ]]; then
    echo "AppDir is missing executable AppRun: $appdir/AppRun" >&2
    exit 1
  fi

  stop_running_install

  rm -rf "$install_root/AppDir"
  rm -f "$install_root/LocalDictate.AppImage"
  mkdir -p "$install_root/AppDir"
  cp -a "$appdir"/. "$install_root/AppDir/"
  chmod +x "$install_root/AppDir/AppRun"

  cat > "$launcher_path" <<EOF
#!/usr/bin/env bash
exec "$install_root/AppDir/AppRun" "\$@"
EOF

  if [[ -f "$appdir/local-dictate.desktop" ]]; then
    desktop_source="$appdir/local-dictate.desktop"
  fi

  if [[ -f "$appdir/local-dictate.svg" ]]; then
    icon_source="$appdir/local-dictate.svg"
  fi

  if [[ -f "$appdir/usr/share/metainfo/local-dictate.appdata.xml" ]]; then
    metainfo_source="$appdir/usr/share/metainfo/local-dictate.appdata.xml"
  fi
fi

chmod +x "$launcher_path"

if [[ -z "$desktop_source" && -f "$(dirname "$0")/../packaging/linux/local-dictate.desktop" ]]; then
  desktop_source="$(dirname "$0")/../packaging/linux/local-dictate.desktop"
fi

if [[ -z "$icon_source" && -f "$(dirname "$0")/../packaging/linux/local-dictate.svg" ]]; then
  icon_source="$(dirname "$0")/../packaging/linux/local-dictate.svg"
fi

if [[ -z "$metainfo_source" && -f "$(dirname "$0")/../packaging/linux/local-dictate.appdata.xml" ]]; then
  metainfo_source="$(dirname "$0")/../packaging/linux/local-dictate.appdata.xml"
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

if [[ -n "$metainfo_source" ]]; then
  install -m 0644 "$metainfo_source" "$metainfo_path"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$applications_root" >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Installed Local Dictate launcher: $desktop_path"
echo "Run it from your app launcher, or start it with: $launcher_path"
