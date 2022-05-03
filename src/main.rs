extern crate hidapi;

use hidapi::{HidApi, HidDevice};
use deku::prelude::*;
use hexlit::hex;

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct BlyncControl {
    header: u8,
    red: u8,
    blue: u8,
    green: u8,
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


fn main() {

    // basic 'blue' message without blink
    // 0000   09 00 02 00 00 08 00 00 00 00 09 00 80 ff 22
    let test_bytes = hex!("00000000000000ff22");

    let mut _bc = BlyncControl::try_from(test_bytes.as_ref()).unwrap();
    
    // _bc.red = 255;
    // _bc.green = 0;
    // _bc.blue = 255;

    // flash
    _bc.flash = 1;
    _bc.speed = 1<<0;

    // _bc.off = 1;

    // _bc.play = 1;
    // _bc.music = 1;
    // _bc.volume = 1;
    // _bc.mute = 0;
    // _bc.repeat = 0;

    match HidApi::new() {
        Ok(api) => {
            for device in api.device_list() {
                println!("{:04x}:{:04x}", device.vendor_id(), device.product_id());

                let dev = api.open(device.vendor_id(), device.product_id()).unwrap();
                let _ = dev.write(_bc.to_bits().unwrap().as_raw_slice());
            }

        },
        Err(e) => {
            eprintln!("Error: {}", e);
        },
    }

}