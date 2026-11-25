# UPS Home Assistant Bridge

A read-only Rust bridge for the CyberPower CST150UC over USB on macOS. It uses shared USB access so macOS can continue monitoring the UPS.

## Setup

Requires Rust and the macOS Command Line Tools. From the clone:

```sh
cargo build --release --locked
./target/release/ups-ha-bridge setup
./target/release/ups-ha-bridge discover
./target/release/ups-ha-bridge read
```

Edit `.env` and put the Mac's LAN address and port in `UPS_LISTEN`. Use a stable LAN address for Home Assistant. To bind all IPv4 interfaces with `0.0.0.0`, set `UPS_ALLOWED_IP` to Home Assistant's source IP to limit access. The endpoint serves plain HTTP and is intended for a trusted LAN.

```sh
./target/release/ups-ha-bridge run
```

Run these commands from the clone so the bridge can read `.env`. Use `ups-ha-bridge doctor` to check local configuration, USB access, and the running HTTP endpoint. Check `GET /ups` from the Home Assistant machine or another permitted LAN client as well. A USB read failure returns HTTP 503. Use `ups-ha-bridge version` or `ups-ha-bridge help` for CLI details.

## Automatic startup

Build the release binary and configure `.env` first. Run one of the following from your macOS user account:

- `./target/release/ups-ha-bridge setup daemon` starts the bridge at system boot and restarts it if it exits. macOS asks for administrator authorization to install the LaunchDaemon. The bridge runs as your regular user.
- `./target/release/ups-ha-bridge setup agent` starts at login and restarts the bridge if it exits.

The service logs to `~/Library/Logs/ups-ha-bridge/service.log`. To switch modes, unload the current service using `launchctl bootout gui/$(id -u)/local.ups-ha-bridge` for an agent or `sudo launchctl bootout system/local.ups-ha-bridge` for a daemon, then remove its plist from `~/Library/LaunchAgents` or `/Library/LaunchDaemons` before installing the other mode.

## Home Assistant

Set `ups_bridge_url` in Home Assistant's `secrets.yaml` to the bridge's LAN URL, including `/ups`. Merge [home-assistant.yaml](home-assistant.yaml) into `configuration.yaml` and check the configuration before restarting Home Assistant.

The REST entities expose battery level, charging state, battery operation, estimated runtime, load percentage, and measured output watts. The Integral sensor estimates cumulative output energy in kWh for the Energy dashboard.
