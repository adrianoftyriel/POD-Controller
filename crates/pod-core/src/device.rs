use std::time::Duration;

use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{Endpoint, MaybeFuture};

use crate::error::{PodError, Result};
use crate::protocol::{self, PacketReassembler};

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

    /// Send one already bulk-framed message (see `protocol::encode_*`) and
    /// wait for the device's reply, reassembling it if it spans multiple
    /// chunks/packets. This does not yet implement the control-transfer
    /// init handshake (`CTRL_REQUEST`) — callers should perform that once
    /// at startup if the device requires it (unconfirmed whether it's
    /// strictly necessary before bulk I/O).
    ///
    /// A single `PodError::Timeout` here means the device didn't answer
    /// within `TIMEOUT` — a possible lockup. Callers should surface this
    /// distinctly rather than retrying in a loop; recovery is a power
    /// cycle, not a resend.
    pub fn transact(&mut self, framed_request: &[u8]) -> Result<Vec<u8>> {
        self.write_raw(framed_request)?;

        let mut reassembler = PacketReassembler::new();
        loop {
            let packet = self.read_raw()?;
            if let Some(payload) = reassembler.push(&packet)?.into_iter().next() {
                return Ok(payload);
            }
        }
    }

    /// Send an already bulk-framed message with no expected reply (the set
    /// messages are fire-and-forget — see docs/PROTOCOL.md).
    pub fn send(&mut self, framed_message: &[u8]) -> Result<()> {
        self.write_raw(framed_message)
    }

    fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        let mut buf = Buffer::new(data.len());
        buf.extend_from_slice(data);
        let completion = self.out_ep.transfer_blocking(buf, TIMEOUT);
        map_transfer_status(completion.status, "OUT")
    }

    fn read_raw(&mut self) -> Result<Vec<u8>> {
        let buf = Buffer::new(protocol::BULK_PACKET_LEN);
        let completion = self.in_ep.transfer_blocking(buf, TIMEOUT);
        map_transfer_status(completion.status, "IN")?;
        Ok(completion.buffer.into_vec())
    }
}

/// Distinguish a timed-out transfer (device unresponsive, possibly wedged)
/// from other transfer failures. `nusb::Endpoint::transfer_blocking` reports
/// a timeout as `TransferError::Cancelled` (it cancels the pending transfer
/// once the deadline passes) — see the crate's `transfer_blocking` docs.
fn map_transfer_status(status: std::result::Result<(), nusb::transfer::TransferError>, direction: &str) -> Result<()> {
    status.map_err(|e| match e {
        nusb::transfer::TransferError::Cancelled => PodError::Timeout,
        other => PodError::Transfer(format!("bulk {direction} failed: {other:?}")),
    })
}
