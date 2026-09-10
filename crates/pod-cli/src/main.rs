use clap::{Parser, Subcommand};
use pod_core::PodDevice;

/// Dev/probe tool for reverse-engineering and testing the POD X3 USB
/// protocol. Not an end-user application.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected POD X3 / X3 Live devices.
    List,
    /// Set a float parameter (WARNING: only index 5 = tone volume is
    /// confirmed correct; anything else is a guess).
    SetFloat {
        #[arg(long)]
        index: u8,
        #[arg(long)]
        value: f32,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::List => {
            let devices = pod_core::find_devices()?;
            if devices.is_empty() {
                println!("No POD X3 / X3 Live found.");
            }
            for d in devices {
                println!(
                    "POD X3{} — vid={:04x} pid={:04x} bus={} addr={}",
                    if d.product_id() == pod_core::protocol::PRODUCT_ID_X3_LIVE {
                        " Live"
                    } else {
                        ""
                    },
                    d.vendor_id(),
                    d.product_id(),
                    d.busnum(),
                    d.device_address(),
                );
            }
        }
        Command::SetFloat { index, value } => {
            let mut dev = PodDevice::open_first()?;
            dev.set_float_param(index, value)?;
            println!("Sent float param index={index} value={value}");
        }
    }
    Ok(())
}
