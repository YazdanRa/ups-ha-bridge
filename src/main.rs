mod ups;

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;
use std::time::Duration;

use tiny_http::{Header, Method, Response, Server};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const HELP: &str = "Usage: ups-ha-bridge <command>
  setup [agent|daemon]  Create .env and optionally install a launchd service
  discover             List supported USB UPS devices
  run                  Serve GET /ups using .env
  doctor               Check .env, USB telemetry, and the HTTP endpoint
  read                 Print current UPS readings as JSON
  version              Print the version
  help                 Show this help";

struct Settings {
    listen: SocketAddr,
    allowed_ip: Option<IpAddr>,
}

impl Settings {
    fn load() -> Result<Self> {
        let values = dotenvy::from_path_iter(".env")
            .map_err(|error| format!("Cannot open .env: {error}. Run setup first."))?
            .collect::<std::result::Result<HashMap<_, _>, _>>()?;
        let listen = values
            .get("UPS_LISTEN")
            .ok_or("UPS_LISTEN is required in .env")?
            .parse()
            .map_err(|error| format!("Invalid UPS_LISTEN address: {error}"))?;
        let allowed_ip = values
            .get("UPS_ALLOWED_IP")
            .filter(|address| !address.is_empty())
            .map(|address| address.parse())
            .transpose()
            .map_err(|error| format!("Invalid UPS_ALLOWED_IP address: {error}"))?;
        Ok(Self { listen, allowed_ip })
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "setup" => setup(None)?,
        [command, mode] if command == "setup" && matches!(mode.as_str(), "agent" | "daemon") => {
            setup(Some(mode))?;
        }
        [command] if command == "discover" => ups::discover()?,
        [command] if command == "run" => run(Settings::load()?)?,
        [command] if command == "doctor" => doctor()?,
        [command] if command == "read" => println!("{}", serde_json::to_string(&ups::read()?)?),
        [command] if matches!(command.as_str(), "version" | "--version" | "-V") => {
            println!("ups-ha-bridge {}", env!("CARGO_PKG_VERSION"));
        }
        [] => println!("{HELP}"),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => println!("{HELP}"),
        _ => return Err(HELP.into()),
    }
    Ok(())
}

fn setup(mode: Option<&str>) -> Result<()> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(".env")
    {
        Ok(mut file) => {
            file.write_all(include_bytes!("../.env.example"))?;
            println!("Created .env. Set the LAN listener before connecting Home Assistant.");
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            println!("Using existing .env.");
        }
        Err(error) => return Err(error.into()),
    }
    if let Some(mode) = mode {
        Settings::load()?;
        let result = Command::new("./install-launchd.sh").arg(mode).status()?;
        if !result.success() {
            return Err(format!("launchd installation exited with {result}").into());
        }
    }
    Ok(())
}

fn doctor() -> Result<()> {
    let settings = Settings::load()?;
    println!("Config OK: listening on {}", settings.listen);
    let usb = ups::read();
    match &usb {
        Ok(_) => println!("USB OK: CyberPower CST150UC telemetry is readable"),
        Err(error) => eprintln!("USB failed: {error}"),
    }

    let mut address = settings.listen;
    match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => address.set_ip(Ipv4Addr::LOCALHOST.into()),
        IpAddr::V6(ip) if ip.is_unspecified() => address.set_ip(Ipv6Addr::LOCALHOST.into()),
        _ => {}
    }
    let http = check_http(address);
    match &http {
        Ok(()) => println!("HTTP OK: {address}/ups returned 200"),
        Err(error) => eprintln!("HTTP failed: {error}. Check whether the bridge is running."),
    }
    usb?;
    http
}

fn check_http(address: SocketAddr) -> Result<()> {
    let timeout = Duration::from_secs(2);
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET /ups HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )?;
    let mut status = String::new();
    BufReader::new(stream).take(128).read_line(&mut status)?;
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(format!("Unexpected HTTP status {:?}", status.trim()).into());
    }
    Ok(())
}

fn run(settings: Settings) -> Result<()> {
    let server = Server::http(settings.listen)?;
    eprintln!("Listening on {}", server.server_addr());
    for request in server.incoming_requests() {
        let (status, body) = if settings.allowed_ip.is_some_and(|allowed| {
            request
                .remote_addr()
                .is_none_or(|peer| !peer.ip().is_loopback() && peer.ip() != allowed)
        }) {
            (403, String::from("{}"))
        } else if request.url() != "/ups" {
            (404, String::from("{}"))
        } else if request.method() != &Method::Get {
            (405, String::from("{}"))
        } else {
            match ups::read() {
                Ok(reading) => (200, serde_json::to_string(&reading)?),
                Err(error) => {
                    eprintln!("UPS unavailable: {error}");
                    (503, String::from(r#"{"available":false}"#))
                }
            }
        };
        let response = Response::from_string(body)
            .with_status_code(status)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        if let Err(error) = request.respond(response) {
            eprintln!("Could not send UPS reading: {error}");
        }
    }
    Ok(())
}
