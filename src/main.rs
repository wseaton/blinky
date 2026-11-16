use std::{thread, time::Duration};

use blinky::{Blynclight, Color, FlashSpeed};
use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, eyre};
use tracing::metadata::LevelFilter;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, PartialEq, Subcommand)]
enum Commands {
    /// Blink a color a number of times, then turn the light off
    Zap {
        #[arg(short, long)]
        color: Color,
        #[arg(short, long, default_value_t = 100)]
        num_blinks: u64,
        #[arg(short, long, value_enum, default_value_t = FlashSpeed::Slow)]
        flash_speed: FlashSpeed,
    },
    /// Show a color, steady or flashing, and leave it on unless a duration is given
    Hold {
        #[arg(short, long)]
        color: Color,

        /// Milliseconds to hold before turning the light off
        #[arg(short, long)]
        duration: Option<u64>,

        /// Flash at this speed
        #[arg(short, long, value_enum)]
        flash_speed: Option<FlashSpeed>,
    },
    /// Turn the light off
    Off,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let light =
        Blynclight::open()?.ok_or_else(|| eyre!("no Blynclight found, is it plugged in?"))?;

    match cli.command {
        Commands::Zap {
            color,
            num_blinks,
            flash_speed,
        } => {
            tracing::info!("Zapping with color: {:?}", color);
            light.flash_color(color, flash_speed, num_blinks)?;
            light.off()?;
        }
        Commands::Hold {
            color,
            duration,
            flash_speed,
        } => {
            tracing::info!("Holding with color: {:?}", color);
            light.show(color, flash_speed)?;
            if let Some(duration) = duration {
                thread::sleep(Duration::from_millis(duration));
                light.off()?;
            }
        }
        Commands::Off => light.off()?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use blinky::{Color, FlashSpeed};
    use clap::{CommandFactory, Parser};

    use crate::{Cli, Commands};

    fn parse(args: &[&str]) -> Result<Commands, clap::Error> {
        Cli::try_parse_from(args).map(|cli| cli.command)
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn zap_defaults_to_100_slow_blinks() {
        assert_eq!(
            parse(&["blinky", "zap", "--color", "red"]).unwrap(),
            Commands::Zap {
                color: Color::RED,
                num_blinks: 100,
                flash_speed: FlashSpeed::Slow,
            }
        );
    }

    #[test]
    fn zap_accepts_short_flags() {
        assert_eq!(
            parse(&["blinky", "zap", "-c", "red", "-n", "3", "-f", "fast"]).unwrap(),
            Commands::Zap {
                color: Color::RED,
                num_blinks: 3,
                flash_speed: FlashSpeed::Fast,
            }
        );
    }

    #[test]
    fn hold_is_steady_and_indefinite_by_default() {
        assert_eq!(
            parse(&["blinky", "hold", "-c", "red"]).unwrap(),
            Commands::Hold {
                color: Color::RED,
                duration: None,
                flash_speed: None,
            }
        );
    }

    #[test]
    fn hold_accepts_duration_and_flash_speed() {
        assert_eq!(
            parse(&[
                "blinky",
                "hold",
                "--color",
                "red",
                "--duration",
                "1500",
                "--flash-speed",
                "medium"
            ])
            .unwrap(),
            Commands::Hold {
                color: Color::RED,
                duration: Some(1500),
                flash_speed: Some(FlashSpeed::Medium),
            }
        );
    }

    #[test]
    fn off_takes_no_arguments() {
        assert_eq!(parse(&["blinky", "off"]).unwrap(), Commands::Off);
        assert!(parse(&["blinky", "off", "--color", "red"]).is_err());
    }

    #[test]
    fn invalid_invocations_are_rejected() {
        assert!(parse(&["blinky"]).is_err());
        assert!(parse(&["blinky", "zap"]).is_err());
        assert!(parse(&["blinky", "hold"]).is_err());
        assert!(parse(&["blinky", "hold", "-c", "notacolor"]).is_err());
        assert!(parse(&["blinky", "hold", "-c", "red", "-f", "warp"]).is_err());
        assert!(parse(&["blinky", "zap", "-c", "red", "-n", "-1"]).is_err());
        assert!(parse(&["blinky", "nope"]).is_err());
    }
}
