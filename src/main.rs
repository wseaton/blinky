extern crate hidapi;
use color_eyre::eyre::Result;
use deku::prelude::*;
use hexlit::hex;
use hidapi::{HidApi, HidDevice};
use tracing::metadata::LevelFilter;

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct BlyncControl {
    header: u8,
    red: u8,
    green: u8, // "blue"
    blue: u8,  // "green"
    #[deku(bits = "1")]
    off: u8,
    #[deku(bits = "1")]
    dim: u8,
    #[deku(bits = "1")]
    flash: u8,
    #[deku(bits = "3")]
    speed: u8,
    #[deku(bits = "2")]
    pad0: u8,
    #[deku(bits = "4")]
    music: u8,
    #[deku(bits = "1")]
    play: u8,
    #[deku(bits = "1")]
    repeat: u8,
    #[deku(bits = "2")]
    pad1: u8,
    #[deku(bits = "4")]
    volume: u8,
    #[deku(bits = "1")]
    mute: u8,
    #[deku(bits = "3")]
    pad2: u8,
    footer: u16,
}

impl BlyncControl {
    fn default() -> BlyncControl {
        // this is what a 'zeroed' struct hexdump would look like.
        // we can use it to start a builder style pattern.
        let zeroed_data = hex!("00000000000000ff22");
        BlyncControl::try_from(zeroed_data.as_ref()).unwrap()
    }

    fn with_color(&mut self, r: u8, g: u8, b: u8) -> &mut BlyncControl {
        self.red = r;
        self.green = g;
        self.blue = b;
        self
    }
}

// typedef struct {
//     unsigned int header: 8;  /* 64:71 Constant: 0x00 */
//     unsigned int red:    8;  /* 56:63 Red color value [-255] */
//     unsigned int blue:   8;  /* 48:55 Blue color value [0-255] */
//     unsigned int green:  8;  /* 40:47 Green color value [0-255] */
//     unsigned int off:    1;  /* 39:39 Set is off, zero is on */
//     unsigned int dim:    1;  /* 38:38 Set is dim, zero is bright */
//     unsigned int flash:  1;  /* 37:37 Set is flash on/off, zero is steady */
//     unsigned int speed:  3;  /* 34:36 Flash speed mask: 1<<0, 1<<1, 1<<2 */
//     unsigned int pad0:   2;  /* 32:33 Unused bits */
//     unsigned int music:  4;  /* 28:31 Stored music index: [0-15] */
//     unsigned int play:   1;  /* 27:27 Set play selected music, zero is stop */
//     unsigned int repeat: 1;  /* 26:26 Set repeats playing music, zero is once */
//     unsigned int pad1:   2;  /* 24:25 Unused bits */
//     unsigned int volume: 4;  /* 20:23 Volume of music: [0-15] */
//     unsigned int mute:   1;  /* 19:19 Set is mute, zero is unmute */
//     unsigned int pad2:   3;  /* 16:18 unused bits */
//     unsigned int footer: 16; /* 00:15 Constant: 0xFF22 */
//  } blynclight_ctrl_t

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    let api = HidApi::new()?;
    let dev = api.open(0x2c0d, 0x0010)?;

    blink_pulse(dev, Some(2))?;

    Ok(())
}

fn blink_pulse(dev: HidDevice, num_blinks: Option<u8>) -> Result<(), color_eyre::Report> {
    let blinks = num_blinks.unwrap_or(3);

    for _ in 1..(blinks + 1) {
        // optional sleep here, it's smoother if left out though.
        // thread::sleep(time::Duration::from_millis(duration));
        for x in 1..(255 * 2) {
            let mut z = x as i16;
            // bidirectional fade hacks
            if z > 255 {
                z = -z;
            }
            set_color(&dev, 255, (z / 2) as u8, z as u8)?;
        }
    }
    turn_off(&dev)?;
    Ok(())
}

/// ~other fun things~
///
/// bc.flash = 1;
/// bc.speed = 1<<0;
/// bc.off = 1;
/// bc.play = 1;
/// bc.music = 1;
/// bc.volume = 1;
/// bc.mute = 0;
/// bc.repeat = 0;
fn set_color(dev: &HidDevice, r: u8, g: u8, b: u8) -> Result<()> {
    let mut bc = BlyncControl::default();
    let x = bc.with_color(r, g, b);
    write_data(dev, &x)?;
    Ok(())
}

fn turn_off(dev: &HidDevice) -> Result<()> {
    let mut bc = BlyncControl::default();
    // color has to be 'zeroed' as well, just because.
    bc.off = 1;
    write_data(dev, &bc)?;
    Ok(())
}

fn write_data(dev: &HidDevice, data: &BlyncControl) -> Result<()> {
    let d = data.to_bits()?;
    let res = dev.write(d.as_raw_slice())?;
    tracing::info!("Wrote {} bytes.", res);
    Ok(())
}
