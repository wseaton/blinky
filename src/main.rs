extern crate hidapi;
use color_eyre::eyre::Result;
use deku::prelude::*;
use hexlit::hex;
use hidapi::{HidApi, HidDevice};
use tracing::metadata::LevelFilter;
use std::{thread, time};

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

    let duration = 50;

    for _ in 1..3 {
        set_color(&dev, 255, 0, 0)?;
        thread::sleep(time::Duration::from_millis(duration));
        set_color(&dev, 0, 0, 255)?;
        thread::sleep(time::Duration::from_millis(duration));
    }

    set_color(&dev, 0, 0, 0)?;

    Ok(())
}


/// ~other fun things~ 
/// bc.flash = 1;
/// bc.speed = 1<<0;
/// bc.off = 1;
/// bc.play = 1;
/// bc.music = 1;
/// bc.volume = 1;
/// bc.mute = 0;
/// bc.repeat = 0;
fn set_color(dev: &HidDevice, r: u8, g: u8, b: u8) -> Result<()> {
    // basic 'blue' message without blink
    // 0000   09 00 02 00 00 08 00 00 00 00 09 00 80 ff 22
    let test_bytes = hex!("00000000000000ff22");
    let mut bc = BlyncControl::try_from(test_bytes.as_ref()).unwrap();

    bc.red = r;
    bc.green = g;
    bc.blue = b;

    let s = bc.to_bits().unwrap();
    tracing::info!("Writing bytes {:?}", s);
    let _ = dev.write(s.as_raw_slice());

    Ok(())
}
