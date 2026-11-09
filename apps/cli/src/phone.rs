//! The phone commands: pairing a phone with the hub, and choosing which phones can reach it.

use std::io::IsTerminal;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, anyhow, bail};
use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::pairing::Device;
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

use crate::args::PhoneCmd;
use crate::run::short;

pub(crate) fn phone_command(client: &Client, command: PhoneCmd) -> anyhow::Result<()> {
    match command {
        PhoneCmd::Pair => pair(client),
        PhoneCmd::List => {
            let devices = client.device_list().map_err(too_old)?;
            if devices.is_empty() {
                println!("no phones are paired. Pair one with `roundhouse phone pair`");
            }
            for device in devices {
                let seen = if device.connections > 0 { "connected".to_string() } else { ago(device.last_seen_ms) };
                println!(
                    "{}  {}  paired {}  last seen {seen}",
                    short(device.id.as_str()),
                    device.name,
                    ago(device.paired_at_ms)
                );
            }
            Ok(())
        }
        PhoneCmd::Revoke { phone } => {
            let devices = client.device_list().map_err(too_old)?;
            let device = find_device(&devices, &phone)?;
            client.device_revoke(&device.id)?;
            println!("revoked {}. It has to pair again to reconnect", device.name);
            Ok(())
        }
    }
}

fn pair(client: &Client) -> anyhow::Result<()> {
    let pairing = client.pairing_start().map_err(too_old)?;
    println!("Scan this with the iPhone's camera to pair it with this hub:\n");
    println!("{}\n", qr_code(&pairing.link.to_string())?);
    println!("Or open this link on the phone:\n{}\n", pairing.link);
    let minutes =
        Duration::from_millis(u64::try_from(pairing.expires_at_ms - now_ms()).unwrap_or(0)).as_secs().div_ceil(60);
    println!("The code works once, for the next {minutes} minutes. Waiting for a phone...");
    match client.pairing_wait(&pairing.code) {
        Ok(device) => {
            println!("Paired with {}", device.name);
            Ok(())
        }
        Err(ClientError::Api(e)) if e.code == ErrorCode::NotFound => bail!(
            "the code expired before a phone paired. A phone has to be on the same network or tailnet as this machine, and a firewall has to let roundhoused accept connections. Run `roundhouse phone pair` for a new code"
        ),
        Err(e) => Err(e).context("waiting for a phone to pair"),
    }
}

/// Light on dark, with the colors set when printing to a terminal, so a camera reads it whatever the terminal's
/// theme.
fn qr_code(link: &str) -> anyhow::Result<String> {
    let code = QrCode::new(link.as_bytes()).context("making a QR code")?;
    let image =
        code.render::<Dense1x2>().dark_color(Dense1x2::Light).light_color(Dense1x2::Dark).quiet_zone(true).build();
    if !std::io::stdout().is_terminal() {
        return Ok(image);
    }
    Ok(image.lines().map(|line| format!("\x1b[97;40m{line}\x1b[0m")).collect::<Vec<_>>().join("\n"))
}

/// A phone's name, ID or ID prefix.
fn find_device<'a>(devices: &'a [Device], query: &str) -> anyhow::Result<&'a Device> {
    let mut matches: Vec<_> = devices.iter().filter(|d| d.name == query || d.id.as_str() == query).collect();
    if matches.is_empty() {
        matches = devices.iter().filter(|d| d.id.as_str().starts_with(query)).collect();
    }
    match matches.as_slice() {
        [one] => Ok(one),
        [] => bail!("no paired phone matches {query}. `roundhouse phone list` shows them"),
        many => bail!("{query} matches {} phones. Use more of the ID", many.len()),
    }
}

fn too_old(error: ClientError) -> anyhow::Error {
    if error.is_unsupported() {
        anyhow!(
            "this hub is too old to pair phones. `roundhouse hub stop` stops it, and the next command starts a new one"
        )
    } else {
        anyhow::Error::new(error)
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn ago(at_ms: i64) -> String {
    let seconds = (now_ms() - at_ms).max(0) / 1000;
    match seconds {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86_400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}
