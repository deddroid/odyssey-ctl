#!/bin/sh
# Install odyssey-ctl for the current user in ~/.local (no root needed),
# except the optional udev rule and i2c-dev module setup (--udev, uses sudo).
#
#   ./install.sh            install binaries, desktop entry and icons
#   ./install.sh --udev     also set up DDC/CI access without root (asks for sudo)
#   ./install.sh --uninstall
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
rule=/etc/udev/rules.d/70-odyssey-ctl.rules
modconf=/etc/modules-load.d/odyssey-ctl.conf
udev=0
uninstall=0
for arg in "$@"; do
    case "$arg" in
        --udev) udev=1 ;;
        --uninstall) uninstall=1 ;;
        -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

# Files may come from a release archive (flat) or a source checkout.
find_file() {
    for p in "$@"; do [ -e "$here/$p" ] && { echo "$here/$p"; return; }; done
    return 1
}

if [ "$uninstall" = 1 ]; then
    rm -f "$prefix/bin/odyssey-ctl" "$prefix/bin/odyssey-ctl-gui" \
          "$prefix/share/applications/odyssey-ctl.desktop" \
          "$HOME/.config/autostart/odyssey-ctl.desktop"
    for s in 16 32 48 64 128 256 512; do
        rm -f "$prefix/share/icons/hicolor/${s}x${s}/apps/odyssey-ctl.png"
    done
    rm -f "$prefix/share/icons/hicolor/scalable/apps/odyssey-ctl.svg"
    if [ -e "$rule" ] || [ -e "$modconf" ]; then
        echo "Removing $rule and $modconf (sudo)"
        sudo rm -f "$rule" "$modconf" && sudo udevadm control --reload
    fi
    echo "Uninstalled. Settings and presets are kept in ${XDG_CONFIG_HOME:-$HOME/.config}/odyssey-ctl."
    exit 0
fi

cli=$(find_file odyssey-ctl target/release/odyssey-ctl) || { echo "odyssey-ctl binary not found (run cargo build --release)" >&2; exit 1; }
gui=$(find_file odyssey-ctl-gui target/release/odyssey-ctl-gui) || gui=""
install -Dm755 "$cli" "$prefix/bin/odyssey-ctl"
if [ -n "$gui" ]; then
    install -Dm755 "$gui" "$prefix/bin/odyssey-ctl-gui"
    desktop=$(find_file odyssey-ctl.desktop packaging/linux/odyssey-ctl.desktop)
    install -Dm644 "$desktop" "$prefix/share/applications/odyssey-ctl.desktop"
    for s in 16 32 48 64 128 256 512; do
        icon=$(find_file "icons/icon-$s.png" "assets/icon-$s.png") && \
            install -Dm644 "$icon" "$prefix/share/icons/hicolor/${s}x${s}/apps/odyssey-ctl.png"
    done
    svg=$(find_file icons/icon.svg assets/icon.svg) && \
        install -Dm644 "$svg" "$prefix/share/icons/hicolor/scalable/apps/odyssey-ctl.svg"
    command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" 2>/dev/null || true
fi

if [ "$udev" = 1 ]; then
    src=$(find_file 70-odyssey-ctl.rules packaging/linux/70-odyssey-ctl.rules)
    conf=$(find_file odyssey-ctl.conf packaging/linux/odyssey-ctl.conf)
    echo "Installing $rule and $modconf (sudo)"
    sudo install -Dm644 "$src" "$rule"
    sudo install -Dm644 "$conf" "$modconf"
    sudo modprobe i2c-dev
    sudo udevadm control --reload
    sudo udevadm trigger --subsystem-match=i2c-dev
else
    echo "Note: without the udev rule the monitor is only accessible as root. Run: $0 --udev"
fi
echo "Installed in $prefix. Make sure $prefix/bin is in your PATH."
