use clap::{Parser, Subcommand};
use pod_core::protocol::{self, ParamNamespace};
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
    /// Set a float parameter (knob) on the block currently at slot/group.
    /// See docs/PROTOCOL.md "Effect knob map" for the slot/group/idx of
    /// each block's knobs.
    SetFloat {
        #[arg(long, default_value_t = 0)]
        tone: u8,
        #[arg(long)]
        slot: u16,
        #[arg(long)]
        group: u16,
        #[arg(long)]
        idx: u16,
        #[arg(long, value_enum, default_value_t = Namespace::Normal)]
        namespace: Namespace,
        #[arg(long)]
        value: f32,
    },
    /// Turn a block on or off.
    SetEnabled {
        #[arg(long, default_value_t = 0)]
        tone: u8,
        #[arg(long)]
        slot: u16,
        #[arg(long)]
        group: u16,
        #[arg(long)]
        enabled: bool,
    },
    /// Select a patch slot as the active patch.
    SelectPatch {
        slot: u32,
    },
    /// Request the EffectDump for a patch slot and print its length.
    Dump {
        slot: u32,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Namespace {
    Normal,
    Mix,
    RealUnit,
}

impl From<Namespace> for ParamNamespace {
    fn from(n: Namespace) -> Self {
        match n {
            Namespace::Normal => ParamNamespace::Normal,
            Namespace::Mix => ParamNamespace::Mix,
            Namespace::RealUnit => ParamNamespace::RealUnit,
        }
    }
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
        Command::SetFloat {
            tone,
            slot,
            group,
            idx,
            namespace,
            value,
        } => {
            let mut dev = PodDevice::open_first()?;
            let msg = protocol::encode_float_set(tone, slot, group, idx, namespace.into(), value);
            dev.send(&msg)?;
            println!("Sent float set tone={tone} slot={slot} group={group} idx={idx} value={value}");
        }
        Command::SetEnabled {
            tone,
            slot,
            group,
            enabled,
        } => {
            let mut dev = PodDevice::open_first()?;
            let msg = protocol::encode_block_enabled(tone, slot, group, enabled);
            dev.send(&msg)?;
            println!("Sent block enabled=({enabled}) tone={tone} slot={slot} group={group}");
        }
        Command::SelectPatch { slot } => {
            let mut dev = PodDevice::open_first()?;
            let msg = protocol::encode_select_patch(slot);
            dev.send(&msg)?;
            println!("Selected patch slot {slot}");
        }
        Command::Dump { slot } => {
            let mut dev = PodDevice::open_first()?;
            let msg = protocol::encode_request_dump(slot);
            let reply = dev.transact(&msg)?;
            println!("Got EffectDump reply: {} bytes", reply.len());
        }
    }
    Ok(())
}
