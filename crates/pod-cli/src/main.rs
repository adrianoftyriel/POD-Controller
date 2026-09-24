use std::path::PathBuf;

use clap::{Parser, Subcommand};
use pod_core::{AmpKnob, Block, PodDevice};

mod serve;

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
    /// Read a patch from the device and save its raw EffectDump blob to a
    /// file (opaque bytes — no internal layout decoding).
    Dump {
        /// Slot index: (bank-1)*4 + channel, A=0..D=3 (e.g. 5A = 0x10).
        #[arg(long)]
        slot: u8,
        #[arg(long)]
        out: PathBuf,
    },
    /// Write a previously dumped EffectDump blob back to a device slot.
    Restore {
        #[arg(long)]
        slot: u8,
        #[arg(long)]
        file: PathBuf,
    },
    /// Make a slot the active/live patch on the device.
    Select {
        #[arg(long)]
        slot: u8,
    },
    /// Set an amp knob (bass/middle/treble/drive/presence/volume) on the
    /// currently loaded patch.
    SetAmp {
        /// Tone 0 or 1.
        #[arg(long)]
        tone: u8,
        #[arg(long)]
        knob: AmpKnob,
        #[arg(long)]
        value: f32,
    },
    /// Enable or disable a block (gate/wah/stomp/amp/eq/comp/mod/delay/reverb)
    /// on the currently loaded patch, at its default chain position.
    Block {
        /// Tone 0 or 1.
        #[arg(long)]
        tone: u8,
        #[arg(long)]
        block: Block,
        #[arg(long)]
        enabled: bool,
    },
    /// Send an arbitrary hex-encoded message and print each raw 64-byte
    /// bulk-IN packet received afterward, unprocessed (no chunk/message
    /// reassembly — there's no in-band end-of-message marker, so a generic
    /// probe can't know how many bytes to expect). Dev/RE tool: for
    /// probing message shapes that aren't yet wrapped in a typed command.
    RawPackets {
        /// Message bytes as hex, no spaces. Leave empty to just listen.
        #[arg(long, default_value = "")]
        message: String,
        /// Number of 64-byte packets to read.
        #[arg(long, default_value_t = 8)]
        count: usize,
    },
    /// Serve a minimal live-view web page showing real patch names read
    /// straight off the device, polling every few seconds. Read-only,
    /// GET-only, single-threaded — a quick way to watch it work, not
    /// something to expose beyond a trusted LAN.
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Bank to show (1-32).
        #[arg(long, default_value_t = 1)]
        bank: u8,
    },
}

fn parse_hex(s: &str) -> anyhow::Result<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        anyhow::bail!("hex string must have an even number of digits");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(anyhow::Error::from))
        .collect()
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
        Command::Dump { slot, out } => {
            let mut dev = PodDevice::open_first()?;
            let patch = dev.read_patch(slot)?;
            std::fs::write(&out, &patch)?;
            println!(
                "Dumped slot {slot} ({} bytes) to {}",
                patch.len(),
                out.display()
            );
        }
        Command::Restore { slot, file } => {
            let patch = std::fs::read(&file)?;
            let mut dev = PodDevice::open_first()?;
            dev.write_patch(slot, &patch)?;
            println!(
                "Restored {} ({} bytes) to slot {slot}",
                file.display(),
                patch.len()
            );
        }
        Command::Select { slot } => {
            let mut dev = PodDevice::open_first()?;
            dev.select_slot(slot)?;
            println!("Selected slot {slot}");
        }
        Command::SetAmp { tone, knob, value } => {
            let mut dev = PodDevice::open_first()?;
            dev.set_amp_knob(tone, knob, value)?;
            println!("Set tone {tone} amp {knob:?} = {value}");
        }
        Command::Block {
            tone,
            block,
            enabled,
        } => {
            let mut dev = PodDevice::open_first()?;
            dev.set_block_enabled(tone, block, enabled)?;
            println!("Set tone {tone} block {block:?} enabled={enabled}");
        }
        Command::RawPackets { message, count } => {
            let mut dev = PodDevice::open_first()?;
            if !message.is_empty() {
                let bytes = parse_hex(&message)?;
                let framed = pod_core::protocol::encode_chunks(&bytes);
                dev.write_raw(&framed)?;
                println!("Sent {} message bytes.", bytes.len());
            }
            for i in 0..count {
                match dev.read_raw() {
                    Ok(packet) => println!(
                        "packet[{i}] ({} bytes): {}",
                        packet.len(),
                        hex::encode(&packet)
                    ),
                    Err(e) => {
                        println!("packet[{i}]: error: {e}");
                        break;
                    }
                }
            }
        }
        Command::Serve { port, bank } => {
            serve::run(port, bank)?;
        }
    }
    Ok(())
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
