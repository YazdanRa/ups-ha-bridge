use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::Serialize;

use crate::Result;

#[derive(Serialize)]
pub struct Reading {
    battery_percent: u8,
    runtime_seconds: u16,
    charging: bool,
    on_battery: bool,
    output_watts: u16,
    load_percent: u8,
}

fn devices(api: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
    api.device_list().filter(|info| {
        info.vendor_id() == 0x0764
            && info.product_id() == 0x0601
            && info.product_string() == Some("CST150UC")
            && info.usage_page() == 0x84
            && info.usage() == 0x04
    })
}

pub fn discover() -> Result<()> {
    let api = HidApi::new()?;
    let mut matches = devices(&api).peekable();
    if matches.peek().is_none() {
        return Err("CyberPower CST150UC not found over USB".into());
    }
    for device in matches {
        println!(
            "CyberPower CST150UC (USB {:04x}:{:04x})",
            device.vendor_id(),
            device.product_id()
        );
    }
    Ok(())
}

pub fn read() -> Result<Reading> {
    let api = HidApi::new()?;
    let mut matches = devices(&api);
    let info = matches
        .next()
        .ok_or("CyberPower CST150UC not found over USB")?;
    if matches.next().is_some() {
        return Err("Expected one CyberPower CST150UC over USB".into());
    }
    let device = info.open_device(&api)?;

    // CST150UC USB feature reports: battery, present status, load and active output power.
    Ok(Reading::from_reports(
        feature(&device, 0x08)?,
        feature(&device, 0x0b)?,
        feature(&device, 0x13)?,
        feature(&device, 0x19)?,
    ))
}

fn feature<const N: usize>(device: &HidDevice, id: u8) -> Result<[u8; N]> {
    let mut report = [0; N];
    report[0] = id;
    let length = device.get_feature_report(&mut report)?;
    if length != N || report[0] != id {
        return Err(format!("Unexpected UPS feature report 0x{id:02x} ({length} bytes)").into());
    }
    Ok(report)
}

impl Reading {
    fn from_reports(battery: [u8; 6], status: [u8; 2], load: [u8; 2], power: [u8; 3]) -> Self {
        let flags = status[1];
        Self {
            battery_percent: battery[1],
            runtime_seconds: u16::from_le_bytes([battery[2], battery[3]]),
            charging: flags & 0x02 != 0,
            on_battery: flags & 0x01 == 0,
            output_watts: u16::from_le_bytes([power[1], power[2]]),
            load_percent: load[1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Reading;

    #[test]
    fn decodes_cst150uc_battery_and_power_reports() {
        let reading = Reading::from_reports(
            [0x08, 100, 0x3d, 0x13, 0x2c, 0x01],
            [0x0b, 0x11],
            [0x13, 11],
            [0x19, 0x6a, 0],
        );
        assert_eq!(reading.battery_percent, 100);
        assert_eq!(reading.runtime_seconds, 4925);
        assert_eq!(reading.output_watts, 106);
        assert_eq!(reading.load_percent, 11);
        assert!(!reading.charging && !reading.on_battery);
    }

    #[test]
    fn distinguishes_charging_from_battery_operation() {
        let from_flags = |flags| Reading::from_reports([0; 6], [0x0b, flags], [0; 2], [0; 3]);
        assert!(from_flags(0x03).charging);
        assert!(!from_flags(0x03).on_battery);
        assert!(from_flags(0x04).on_battery);
        assert!(!from_flags(0x04).charging);
    }
}
