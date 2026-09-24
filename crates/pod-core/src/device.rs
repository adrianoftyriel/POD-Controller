use std::time::Duration;

use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{Endpoint, MaybeFuture};

use crate::error::{PodError, Result};
use crate::params::{AmpKnob, Block};
use crate::protocol::{self, ChunkReassembler};

const TIMEOUT: Duration = Duration::from_secs(2);

/// A connection to a POD X3's control interface.
pub struct PodDevice {
    // Kept alive so the interface claim isn't released.
    _interface: nusb::Interface,
    out_ep: Endpoint<Bulk, Out>,
    in_ep: Endpoint<Bulk, In>,
}

/// Enumerate connected POD X3 / X3 Live devices.
pub fn find_devices() -> Result<Vec<nusb::DeviceInfo>> {
    let devices: Vec<_> = nusb::list_devices()
        .wait()?
        .filter(|d| {
            d.vendor_id() == protocol::VENDOR_ID
                && (d.product_id() == protocol::PRODUCT_ID_X3
                    || d.product_id() == protocol::PRODUCT_ID_X3_LIVE)
        })
        .collect();
    Ok(devices)
}

impl PodDevice {
    /// Open the first connected POD X3 / X3 Live and claim its control
    /// interface.
    pub fn open_first() -> Result<Self> {
        let info = find_devices()?
            .into_iter()
            .next()
            .ok_or(PodError::DeviceNotFound)?;
        Self::open(&info)
    }

    pub fn open(info: &nusb::DeviceInfo) -> Result<Self> {
        let device = info.open().wait()?;
        let interface = device.claim_interface(protocol::CONTROL_INTERFACE).wait()?;
        let out_ep = interface.endpoint::<Bulk, Out>(protocol::BULK_OUT_EP)?;
        let in_ep = interface.endpoint::<Bulk, In>(protocol::BULK_IN_EP)?;
        Ok(Self {
            _interface: interface,
            out_ep,
            in_ep,
        })
    }

    /// Send one bulk-framed message and wait for the device's reply,
    /// reassembling it if it spans multiple packets. This does not yet
    /// implement the control-transfer init handshake (`CTRL_REQUEST`) —
    /// callers should perform that once at startup if the device requires
    /// it (unconfirmed whether it's strictly necessary before bulk I/O).
    pub fn transact(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        self.write_raw(request)?;

        let mut reassembler = ChunkReassembler::new();
        loop {
            let packet = self.read_raw()?;
            if let Some(payload) = reassembler.push(&packet)? {
                return Ok(payload);
            }
        }
    }

    fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        let mut buf = Buffer::new(data.len());
        buf.extend_from_slice(data);
        let completion = self.out_ep.transfer_blocking(buf, TIMEOUT);
        completion
            .status
            .map_err(|e| PodError::Transfer(format!("bulk OUT failed: {e:?}")))?;
        Ok(())
    }

    fn read_raw(&mut self) -> Result<Vec<u8>> {
        let buf = Buffer::new(protocol::BULK_PACKET_LEN);
        let completion = self.in_ep.transfer_blocking(buf, TIMEOUT);
        completion
            .status
            .map_err(|e| PodError::Transfer(format!("bulk IN failed: {e:?}")))?;
        Ok(completion.buffer.into_vec())
    }

    /// Set a float parameter on the currently loaded patch. Only
    /// `param_index = 5` (tone volume) is confirmed correct — see
    /// `docs/PROTOCOL.md`.
    pub fn set_float_param(&mut self, param_index: u8, value: f32) -> Result<()> {
        let msg = protocol::encode_float_param(param_index, value);
        self.write_raw(&msg)
    }

    /// Read the EffectDump (opaque 4096-byte patch blob) for `slot`.
    /// Slot numbering is `(bank-1)*4 + channel`, A=0..D=3 — see
    /// `docs/PROTOCOL.md` "Slot numbering".
    pub fn read_patch(&mut self, slot: u8) -> Result<Vec<u8>> {
        let request = protocol::encode_request_dump(slot);
        let reply = self.transact(&request)?;
        protocol::decode_effect_dump(&reply).map(|patch| patch.to_vec())
    }

    /// Write a raw, opaque EffectDump blob (as produced by [`Self::read_patch`])
    /// to `slot`, and wait for the device's ack.
    pub fn write_patch(&mut self, slot: u8, patch: &[u8]) -> Result<()> {
        let message = protocol::encode_write_dump(slot, patch)?;
        let ack = self.transact(&message)?;
        if ack.first().copied() != Some(protocol::MessageType::ConfigCmd as u8) {
            return Err(PodError::Protocol(format!(
                "expected ConfigCmd ack after patch write, got {ack:02x?}"
            )));
        }
        Ok(())
    }

    /// Make `slot` the active/live patch on the device. Fire-and-forget:
    /// no reply is documented for this message.
    pub fn select_slot(&mut self, slot: u8) -> Result<()> {
        let message = protocol::encode_select_slot(slot);
        self.write_raw(&message)
    }

    /// Set an amp knob on `tone` (0 or 1) of the currently loaded patch.
    pub fn set_amp_knob(&mut self, tone: u8, knob: AmpKnob, value: f32) -> Result<()> {
        let msg = protocol::encode_float_set(
            tone,
            AmpKnob::SLOT,
            AmpKnob::GROUP,
            knob.idx(),
            protocol::namespace::NORMAL,
            value,
        );
        self.write_raw(&msg)
    }

    /// Enable or disable `block` on `tone` (0 or 1) of the currently loaded
    /// patch. Addresses the block at its *default* chain position — see
    /// [`Block::slot_group`] for the caveat about blocks moved pre/post-amp.
    pub fn set_block_enabled(&mut self, tone: u8, block: Block, enabled: bool) -> Result<()> {
        for &(slot, group) in block.slot_group() {
            let msg = protocol::encode_int_set(
                tone,
                protocol::int_set::BLOCK_ENABLED,
                slot,
                group,
                enabled as u32,
            );
            self.write_raw(&msg)?;
        }
        Ok(())
    }
}
