use std::{fmt, str::FromStr, thread, time::Duration};

use clap::ValueEnum;
use color_eyre::eyre::{Result, WrapErr};
use deku::prelude::*;
use hidapi::{HidApi, HidDevice};
use palette::named;

const EMBRAVA_VENDOR_ID: u16 = 0x2c0d;
const BLYNCLIGHT_PRODUCT_ID: u16 = 0x0010;
const ZAP_HALF_PERIOD: Duration = Duration::from_millis(50);

#[derive(Debug, PartialEq, DekuWrite)]
#[deku(endian = "big")]
/// BlyncControl represents a structure used for controlling Blync light.
/// Each field corresponds to a feature of the Blync light that can be controlled.
struct BlyncControl {
    header: u8,
    red: u8,   // Red color value [0-255]
    green: u8, // Green color value [0-255]
    blue: u8,  // Blue color value [0-255]
    #[deku(bits = 1)]
    off: bool, // Set is off, zero is on
    #[deku(bits = 1)]
    dim: bool, // Set is dim, zero is bright
    #[deku(bits = 1)]
    flash: bool, // Set is flash on/off, zero is steady
    #[deku(bits = 3, pad_bits_after = "2")]
    speed: u8, // Flash speed mask: 1<<0, 1<<1, 1<<2
    #[deku(bits = 4)]
    music: u8, // Stored music index: [0-15]
    #[deku(bits = 1)]
    play: bool, // Set play selected music, zero is stop
    #[deku(bits = 1, pad_bits_after = "2")]
    repeat: bool, // Set repeats playing music, zero is once
    #[deku(bits = 4)]
    volume: u8, // Volume of music: [0-15]
    #[deku(bits = 1, pad_bits_after = "3")]
    mute: bool, // Set is mute, zero is unmute
    footer: u16,
}

impl BlyncControl {
    const FOOTER: u16 = 0xff22;

    /// Constructs a new `BlyncControl` with color fields set to the given color.
    fn solid(color: Color) -> Self {
        Self {
            header: 0,
            red: color.red,
            green: color.green,
            blue: color.blue,
            off: false,
            dim: false,
            flash: false,
            speed: 0,
            music: 0,
            play: false,
            repeat: false,
            volume: 0,
            mute: false,
            footer: Self::FOOTER,
        }
    }

    fn flashing(color: Color, speed: FlashSpeed) -> Self {
        Self {
            flash: true,
            speed: speed.mask(),
            ..Self::solid(color)
        }
    }

    fn off() -> Self {
        // color has to be 'zeroed' as well, just because.
        Self {
            off: true,
            ..Self::solid(Color::BLACK)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    pub const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };
    pub const RED: Self = Self {
        red: 255,
        green: 0,
        blue: 0,
    };
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnknownColor(String);

impl fmt::Display for UnknownColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown color name: {}", self.0)
    }
}

impl std::error::Error for UnknownColor {}

impl FromStr for Color {
    type Err = UnknownColor;

    /// Converts a named color to a `Color`.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        named::from_str(name)
            .map(|c| Self {
                red: c.red,
                green: c.green,
                blue: c.blue,
            })
            .ok_or_else(|| UnknownColor(name.to_owned()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FlashSpeed {
    Slow,
    Medium,
    Fast,
}

impl FlashSpeed {
    fn mask(self) -> u8 {
        match self {
            FlashSpeed::Slow => 1 << 0,
            FlashSpeed::Medium => 1 << 1,
            FlashSpeed::Fast => 1 << 2,
        }
    }
}

pub struct Blynclight {
    dev: HidDevice,
}

impl Blynclight {
    /// Opens the light, or returns `None` when none is plugged in.
    pub fn open() -> Result<Option<Self>> {
        let api = HidApi::new().wrap_err("failed to initialize the HID API")?;
        let present = api.device_list().any(|info| {
            info.vendor_id() == EMBRAVA_VENDOR_ID && info.product_id() == BLYNCLIGHT_PRODUCT_ID
        });
        if !present {
            return Ok(None);
        }
        let dev = api
            .open(EMBRAVA_VENDOR_ID, BLYNCLIGHT_PRODUCT_ID)
            .wrap_err("failed to open the Blynclight")?;
        Ok(Some(Self { dev }))
    }

    /// Shows a color, steady or flashing, until told otherwise.
    pub fn show(&self, color: Color, flash_speed: Option<FlashSpeed>) -> Result<()> {
        let control = match flash_speed {
            Some(speed) => BlyncControl::flashing(color, speed),
            None => BlyncControl::solid(color),
        };
        self.write_data(&control)
    }

    /// Turns off the device.
    pub fn off(&self) -> Result<()> {
        tracing::info!("Turning off!");
        self.write_data(&BlyncControl::off())
    }

    /// Flashes the color of the device the given number of times at the specified frequency.
    pub fn flash_color(&self, color: Color, frequency: FlashSpeed, num_times: u64) -> Result<()> {
        let flash = BlyncControl::flashing(color, frequency);
        let off = BlyncControl::off();
        for _ in 0..num_times {
            self.write_data(&flash)?;
            thread::sleep(ZAP_HALF_PERIOD);
            self.write_data(&off)?;
            thread::sleep(ZAP_HALF_PERIOD);
        }
        Ok(())
    }

    /// Writes the provided BlyncControl data to the device.
    fn write_data(&self, data: &BlyncControl) -> Result<()> {
        let bytes = data.to_bytes()?;
        let written = self.dev.write(&bytes)?;
        tracing::debug!("Wrote: {:02x?}, it was {} bytes.", bytes, written);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use deku::DekuContainerWrite;

    use crate::{BlyncControl, Color, FlashSpeed, UnknownColor};

    #[test]
    fn solid_packet_carries_color_and_footer() {
        let color = Color {
            red: 0x12,
            green: 0x34,
            blue: 0x56,
        };
        assert_eq!(
            BlyncControl::solid(color).to_bytes().unwrap(),
            [0x00, 0x12, 0x34, 0x56, 0x00, 0x00, 0x00, 0xff, 0x22]
        );
    }

    #[test]
    fn off_packet_sets_only_the_off_bit() {
        assert_eq!(
            BlyncControl::off().to_bytes().unwrap(),
            [0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0xff, 0x22]
        );
    }

    #[test]
    fn flashing_packet_sets_flash_bit_and_speed_mask() {
        for (speed, control_byte) in [
            (FlashSpeed::Slow, 0x24),
            (FlashSpeed::Medium, 0x28),
            (FlashSpeed::Fast, 0x30),
        ] {
            assert_eq!(
                BlyncControl::flashing(Color::RED, speed)
                    .to_bytes()
                    .unwrap(),
                [0x00, 0xff, 0x00, 0x00, control_byte, 0x00, 0x00, 0xff, 0x22],
                "{speed:?}"
            );
        }
    }

    #[test]
    fn every_field_lands_in_its_documented_bits() {
        let control = BlyncControl {
            dim: true,
            music: 0b1010,
            play: true,
            repeat: true,
            volume: 0b0110,
            mute: true,
            ..BlyncControl::solid(Color::BLACK)
        };
        assert_eq!(
            control.to_bytes().unwrap(),
            [0x00, 0x00, 0x00, 0x00, 0x40, 0xac, 0x68, 0xff, 0x22]
        );
    }

    #[test]
    fn flash_speed_masks_are_distinct_single_bits() {
        assert_eq!(FlashSpeed::Slow.mask(), 0b001);
        assert_eq!(FlashSpeed::Medium.mask(), 0b010);
        assert_eq!(FlashSpeed::Fast.mask(), 0b100);
    }

    #[test]
    fn color_parses_css_names() {
        assert_eq!("red".parse(), Ok(Color::RED));
        assert_eq!(
            "rebeccapurple".parse(),
            Ok(Color {
                red: 102,
                green: 51,
                blue: 153
            })
        );
        assert_eq!("black".parse(), Ok(Color::BLACK));
    }

    #[test]
    fn color_rejects_unknown_names() {
        assert_eq!(
            "notacolor".parse::<Color>(),
            Err(UnknownColor("notacolor".to_owned()))
        );
        assert_eq!("".parse::<Color>(), Err(UnknownColor(String::new())));
        assert_eq!(
            UnknownColor("mauve-ish".to_owned()).to_string(),
            "unknown color name: mauve-ish"
        );
    }
}
