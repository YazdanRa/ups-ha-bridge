#!/bin/sh
set -eu

test "$(id -u)" -ne 0 || { echo "Run this script as your regular macOS user." >&2; exit 1; }

repo_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)
label=local.ups-ha-bridge
binary="$repo_dir/target/release/ups-ha-bridge"
log_dir="$HOME/Library/Logs/ups-ha-bridge"
agent_path="$HOME/Library/LaunchAgents/$label.plist"
daemon_path="/Library/LaunchDaemons/$label.plist"

case "${1:-}" in
    agent)
        test ! -e "$daemon_path" || { echo "Remove the existing LaunchDaemon first." >&2; exit 1; }
        domain="gui/$(id -u)"
        destination="$agent_path"
        mkdir -p "$HOME/Library/LaunchAgents"
        ;;
    daemon)
        test ! -e "$agent_path" || { echo "Remove the existing LaunchAgent first." >&2; exit 1; }
        domain=system
        destination="$daemon_path"
        ;;
    *)
        echo "Usage: $0 agent|daemon" >&2
        exit 2
        ;;
esac

test -x "$binary" || { echo "Build first with cargo build --release --locked." >&2; exit 1; }
test -r "$repo_dir/.env" || { echo "Create .env from .env.example first." >&2; exit 1; }

mkdir -p "$log_dir"
temp_dir=$(mktemp -d)
trap 'rm -r "$temp_dir"' EXIT
plist="$temp_dir/$label.plist"

plutil -create xml1 "$plist"
plutil -insert Label -string "$label" "$plist"
plutil -insert ProgramArguments -json '[]' "$plist"
plutil -insert ProgramArguments.0 -string "$binary" "$plist"
plutil -insert ProgramArguments.1 -string run "$plist"
plutil -insert WorkingDirectory -string "$repo_dir" "$plist"
plutil -insert KeepAlive -bool YES "$plist"
plutil -insert StandardErrorPath -string "$log_dir/service.log" "$plist"
if [ "$domain" = system ]; then
    plutil -insert UserName -string "$(id -un)" "$plist"
fi
plutil -lint "$plist"

run() {
    if [ "$domain" = system ]; then
        sudo "$@"
    else
        "$@"
    fi
}

if [ "$domain" = system ]; then
    run install -o root -g wheel -m 0644 "$plist" "$destination"
else
    run install -m 0644 "$plist" "$destination"
fi
run launchctl enable "$domain/$label"
if run launchctl print "$domain/$label" >/dev/null 2>&1; then
    run launchctl bootout "$domain/$label"
fi
run launchctl bootstrap "$domain" "$destination"
printf 'Installed %s/%s\nLog: %s/service.log\n' "$domain" "$label" "$log_dir"
