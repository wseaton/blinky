extern crate hidapi;
use std::num;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use deku::prelude::*;
use eyre::WrapErr;
use hexlit::hex;
use hidapi::{HidApi, HidDevice};
use palette::{encoding::Srgb, named, rgb::Rgb, Srgb as SrgbColorSpace};
use tracing::metadata::LevelFilter;

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
/// BlyncControl represents a structure used for controlling Blync light.
/// Each field corresponds to a feature of the Blync light that can be controlled.
pub struct BlyncControl {
    header: u8,
    red: u8,   // Red color value [0-255]
    green: u8, // Green color value [0-255]
    blue: u8,  // Blue color value [0-255]
    #[deku(bits = "1")]
    off: u8, // Set is off, zero is on
    #[deku(bits = "1")]
    dim: u8, // Set is dim, zero is bright
    #[deku(bits = "1")]
    flash: u8, // Set is flash on/off, zero is steady
    #[deku(bits = "3")]
    speed: u8, // Flash speed mask: 1<<0, 1<<1, 1<<2
    #[deku(bits = "2")]
    pad0: u8, // Unused bits
    #[deku(bits = "4")]
    music: u8, // Stored music index: [0-15]
    #[deku(bits = "1")]
    play: u8, // Set play selected music, zero is stop
    #[deku(bits = "1")]
    repeat: u8, // Set repeats playing music, zero is once
    #[deku(bits = "2")]
    pad1: u8, // Unused bits
    #[deku(bits = "4")]
    volume: u8, // Volume of music: [0-15]
    #[deku(bits = "1")]
    mute: u8, // Set is mute, zero is unmute
    #[deku(bits = "3")]
    pad2: u8, // Unused bits
    footer: u16,
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Program to upload the vault secret for a DET namespace into vault automatically
    ///
    /// Dependencies:
    ///    - vault, oc (oc login must be run before running this program) or a valid kubeconfig
    ///    - environment variables: ASTRONOMER_TOKEN, VAULT_SECRET_ID
    Zap {
        #[clap(short, long)]
        color: String,
        #[clap(short, long, default_value = "100")]
        num_blinks: u64,
        #[clap(short, long, value_parser = parse_flashspeed, default_value = "fast")]
        flash_speed: FlashSpeed,
    },
}

fn parse_flashspeed(input: &str) -> Result<FlashSpeed> {
    let res = match input {
        "fast" => Ok(FlashSpeed::Fast),
        "medium" => Ok(FlashSpeed::Medium),
        "slow" => Ok(FlashSpeed::Slow),
        _ => panic!("Invalid flash speed: {}", input),
    };
    res
}

impl BlyncControl {
    /// Constructs a new `BlyncControl` with all fields set to zero.
    fn zeroed() -> Result<BlyncControl> {
        let zeroed_data = hex!("00000000000000ff22");
        BlyncControl::try_from(zeroed_data.as_ref())
            .wrap_err_with(|| format!("Failed to parse zeroed data: {:?}", zeroed_data))
    }

    /// Constructs a new `BlyncControl` with color fields set to the given RGB values.
    fn with_color(r: u8, g: u8, b: u8) -> Result<BlyncControl> {
        let mut control = Self::zeroed()?;
        control.red = r;
        control.green = g;
        control.blue = b;
        Ok(control)
    }
}
fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    println!("Printing all available hid devices:");

    match HidApi::new() {
        Ok(api) => {
            for device in api.device_list() {
                tracing::info!(
                    "{:04x}:{:04x} {}",
                    device.vendor_id(),
                    device.product_id(),
                    device.product_string().unwrap_or("")
                );
            }
        }
        Err(e) => {
            tracing::error!("Error: {}", e);
        }
    }

    let cli = Cli::parse();

    let api = HidApi::new()?;
    let dev = api.open(0x2c0d, 0x0010)?;

    match cli.command {
        Some(Commands::Zap {
            color,
            num_blinks,
            flash_speed,
        }) => {
            tracing::info!("Zapping with color: {}", color);
            let c = color_from_name(&color).expect("Failed to get color from name");
            flash_color(&dev, c.0, c.1, c.2, flash_speed, num_blinks)?;
            turn_off(&dev)?;
        }
        None => {
            tracing::info!("No command provided, doing nothing");
        }
    }

    Ok(())
}

// A constant representing the max color value
const MAX_COLOR: u16 = 255;

fn blink_pulse(dev: HidDevice, num_blinks: Option<u8>) -> Result<(), color_eyre::Report> {
    let blinks = num_blinks.unwrap_or(2);

    for _ in 1..=blinks {
        for x in 1..=(MAX_COLOR * 2) {
            let mut z = x as i16;
            if z > MAX_COLOR as i16 {
                z = -z;
            }
            set_color(
                &dev,
                MAX_COLOR.try_into().expect("We know this will fit"),
                0_u8,
                z as u8,
            )?;
        }
    }
    turn_off(&dev)?;
    Ok(())
}

/// Sets the color of the device using the provided RGB values.
fn set_color(dev: &HidDevice, r: u8, g: u8, b: u8) -> Result<()> {
    let bc = BlyncControl::with_color(r, g, b)?;
    write_data(dev, &bc)
}

/// Turns off the device.
fn turn_off(dev: &HidDevice) -> Result<()> {
    let mut bc = BlyncControl::zeroed()?;
    // color has to be 'zeroed' as well, just because.
    bc.off = 1;
    tracing::info!("Turning off!");
    write_data(dev, &bc)
}

/// Writes the provided BlyncControl data to the device.
fn write_data(dev: &HidDevice, data: &BlyncControl) -> Result<()> {
    let d = data.to_bits()?;
    let res = dev.write(d.as_raw_slice())?;
    tracing::debug!("Wrote: {:x}, it was {} bytes.", &d, res);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum FlashSpeed {
    Slow,
    Medium,
    Fast,
}

// implt To u8 for FlashSpeed
impl From<FlashSpeed> for u8 {
    fn from(f: FlashSpeed) -> u8 {
        f.to_u8()
    }
}

impl FlashSpeed {
    fn to_u8(&self) -> u8 {
        match self {
            FlashSpeed::Slow => 1 << 0,
            FlashSpeed::Medium => 1 << 1,
            FlashSpeed::Fast => 1 << 2,
        }
    }
}

/// Flashes the color of the device using the provided RGB values and the specified frequency.
fn flash_color(
    dev: &HidDevice,
    r: u8,
    g: u8,
    b: u8,
    frequency: FlashSpeed,
    num_times: u64,
) -> Result<()> {
    let bc = BlyncControl::with_color(r, g, b)?;
    let mut flash_bc = bc;
    flash_bc.flash = 1;
    flash_bc.speed = frequency.into();
    for _ in 0..num_times {
        write_data(dev, &flash_bc)?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        turn_off(dev)?;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

fn strobe(dev: &HidDevice, r: u8, g: u8, b: u8, delay: u64, times: u32) -> Result<()> {
    for _ in 0..times {
        set_color(dev, r, g, b)?;
        std::thread::sleep(std::time::Duration::from_millis(delay));
        turn_off(dev)?;
        std::thread::sleep(std::time::Duration::from_millis(delay));
    }
    Ok(())
}

fn transition_colors(dev: &HidDevice, start: (u8, u8, u8), end: (u8, u8, u8)) -> Result<()> {
    let steps = 100; // Define how smooth you want the transition
    for i in 0..steps {
        let r = start.0 + ((end.0 - start.0) * i / steps);
        let g = start.1 + ((end.1 - start.1) * i / steps);
        let b = start.2 + ((end.2 - start.2) * i / steps);
        set_color(dev, r, g, b)?;
        std::thread::sleep(std::time::Duration::from_millis(100)); // Adjust for the speed of transition
    }
    Ok(())
}

/// Converts a named color to a RGB tuple. If the color name isn't recognized, returns `None`.
fn color_from_name(name: &str) -> Option<(u8, u8, u8)> {
    named::from_str(name).map(|c| (c.red, c.green, c.blue))
}
