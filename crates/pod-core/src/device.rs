use std::collections::VecDeque;
use std::time::{Duration, Instant};

use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{Endpoint, MaybeFuture};

use crate::error::{PodError, Result};
use crate::params::{AmpKnob, Block};
use crate::protocol::{self, ChunkReassembler};

const TIMEOUT: Duration = Duration::from_secs(2);

/// Bulk-IN transfers kept submitted at all times.
///
/// The POD only accepts a bulk OUT message while the host has a bulk-IN
/// transfer pending: with none queued it takes one message, then NAKs every
/// OUT after it until an IN read is submitted (confirmed 2026-09-25, 4/4
/// trials each way, see docs/PROTOCOL.md "Keep a bulk-IN read pending").
/// This was the "wedge" that looked like a flaky cable. A few extra
/// transfers give headroom for unsolicited messages arriving while nobody
/// is reading.
const IN_QUEUE_DEPTH: usize = 8;

/// A connection to a POD X3's control interface.
pub struct PodDevice {
    // Kept alive so the interface claim isn't released.
    _interface: nusb::Interface,
    out_ep: Endpoint<Bulk, Out>,
    in_ep: Endpoint<Bulk, In>,
    /// Raw IN packets that have arrived but not been consumed yet.
    inbox: VecDeque<Vec<u8>>,
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
        let mut dev = Self {
            _interface: interface,
            out_ep,
            in_ep,
            inbox: VecDeque::new(),
        };
        dev.fill_in_queue();
        Ok(dev)
    }

    fn fill_in_queue(&mut self) {
        while self.in_ep.pending() < IN_QUEUE_DEPTH {
            let buf = self.in_ep.allocate(protocol::BULK_PACKET_LEN);
            self.in_ep.submit(buf);
        }
    }

    /// Wait up to `timeout` for one IN transfer to complete, move its data
    /// to the inbox and resubmit the buffer, so the queue never runs dry.
    /// Returns whether a transfer completed.
    fn poll_in(&mut self, timeout: Duration) -> Result<bool> {
        let Some(completion) = self.in_ep.wait_next_complete(timeout) else {
            return Ok(false);
        };
        let status = completion.status;
        if status.is_ok() && !completion.buffer.is_empty() {
            self.inbox.push_back(completion.buffer.to_vec());
        }
        self.in_ep.submit(completion.buffer);
        status.map_err(|e| PodError::Transfer(format!("bulk IN failed: {e:?}")))?;
        Ok(true)
    }

    /// Collect every IN transfer that has already completed, without
    /// blocking.
    fn poll_in_nowait(&mut self) -> Result<()> {
        while self.poll_in(Duration::ZERO)? {}
        Ok(())
    }

    /// Take every raw IN packet received so far that no reply has claimed,
    /// e.g. the POD's unsolicited `04`/`13` block announcements.
    pub fn take_unsolicited(&mut self) -> Result<Vec<Vec<u8>>> {
        self.poll_in_nowait()?;
        Ok(self.inbox.drain(..).collect())
    }

    /// Send one bulk-framed message and wait for the reply whose type byte
    /// is `reply_type` and subcommand byte is `reply_sub`, reading exactly
    /// `reply_len` bytes of it. Callers must know the reply length up front
    /// — there's no in-band end-of-message marker (see [`ChunkReassembler`]).
    ///
    /// A message starts at a chunk flagged [`protocol::FLAG_FIRST`]. The POD
    /// sends unsolicited messages (e.g. `04`/`13` block announcements, which
    /// arrive before the ack of a tone push), so any message with a
    /// different type/sub is skipped, as is anything already waiting in the
    /// inbox. This does not implement the control-transfer init handshake
    /// (`CTRL_REQUEST`), which bulk I/O works without.
    pub fn transact(
        &mut self,
        request: &[u8],
        reply_type: u8,
        reply_sub: u8,
        reply_len: usize,
    ) -> Result<Vec<u8>> {
        for stale in self.take_unsolicited()? {
            tracing::debug!("dropping unsolicited packet {stale:02x?}");
        }
        self.write_raw(request)?;

        let mut reassembler = ChunkReassembler::new();
        let mut message = Vec::with_capacity(reply_len);
        let mut started = false;
        loop {
            let packet = self.read_raw()?;
            if packet.get(2).is_some_and(|f| f & protocol::FLAG_FIRST != 0) {
                started = true;
                reassembler = ChunkReassembler::new();
                message.clear();
            }
            if !started {
                continue;
            }
            message.extend(reassembler.push(&packet)?);
            if message.len() >= 8 && (message[0] != reply_type || message[7] != reply_sub) {
                tracing::debug!("skipping unsolicited message {:02x?}", &message[..8]);
                started = false;
                continue;
            }
            if message.len() >= reply_len {
                message.truncate(reply_len);
                return Ok(message);
            }
        }
    }

    /// Send already chunk-framed bytes with no attempt to read a reply.
    /// Low-level primitive for RE/probing — prefer [`Self::transact`] or a
    /// typed method when the message shape is known.
    ///
    /// The data goes out as one 64-byte transfer per USB packet, waiting for
    /// each to complete before sending the next. The POD silently drops a
    /// message sent faster than that: a 4176-byte patch write as a single
    /// transfer, or as 256-byte transfers, is never acked, while 128- and
    /// 64-byte transfers are (2026-09-25). Gearbox sends 64 bytes at a time.
    pub fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        // Resubmit anything that completed so the IN queue is full before
        // the POD sees this message (see IN_QUEUE_DEPTH).
        self.poll_in_nowait()?;
        for packet in data.chunks(protocol::BULK_PACKET_LEN) {
            let mut buf = Buffer::new(packet.len());
            buf.extend_from_slice(packet);
            let completion = self.out_ep.transfer_blocking(buf, TIMEOUT);
            completion.status.map_err(|e| match e {
                // transfer_blocking cancels on timeout: the POD NAKed the
                // packet for the whole TIMEOUT.
                nusb::transfer::TransferError::Cancelled => PodError::Timeout,
                e => PodError::Transfer(format!("bulk OUT failed: {e:?}")),
            })?;
        }
        Ok(())
    }

    /// Read one raw [`protocol::BULK_PACKET_LEN`]-byte bulk-IN packet, with
    /// no chunk/message reassembly. Low-level primitive for RE/probing.
    pub fn read_raw(&mut self) -> Result<Vec<u8>> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(packet) = self.inbox.pop_front() {
                return Ok(packet);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(PodError::Timeout);
            }
            self.poll_in(left)?;
        }
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
        let reply = self.transact(
            &request,
            protocol::MessageType::EffectDump as u8,
            protocol::config_cmd::EFFECT_DUMP_REPLY,
            8 + protocol::EFFECT_DUMP_LEN,
        )?;
        protocol::decode_effect_dump(&reply).map(|patch| patch.to_vec())
    }

    /// Send `message` and wait for the POD's 12-byte `02`/`03` ack (seen
    /// after patch writes and tone pushes, sometimes split across two
    /// chunks).
    fn send_acked(&mut self, message: &[u8]) -> Result<()> {
        self.transact(
            message,
            protocol::MessageType::ConfigCmd as u8,
            protocol::config_cmd::ACK,
            protocol::ACK_LEN,
        )?;
        Ok(())
    }

    /// Write a raw, opaque EffectDump blob (as produced by [`Self::read_patch`])
    /// to `slot`, and wait for the device's ack. Confirmed on hardware
    /// (2026-09-25): the stored patch reads back byte for byte.
    pub fn write_patch(&mut self, slot: u8, patch: &[u8]) -> Result<()> {
        let message = protocol::encode_write_dump(slot, patch)?;
        self.send_acked(&message)
    }

    /// Push one tone's 2048-byte block (`tone` 0 or 1) into the edit buffer
    /// and wait for the ack.
    pub fn push_tone(&mut self, tone: u8, block: &[u8]) -> Result<()> {
        let message = protocol::encode_push_tone(tone, block)?;
        self.send_acked(&message)
    }

    /// Load the patch stored in `slot` into the edit buffer, the way Gearbox
    /// does: `02`/`27` with the slot, both tone blocks pushed (each acked),
    /// then Tone 1 selected for editing. Gearbox never sends `02`/`27`
    /// without the pushes, and the POD doesn't answer a bare one, so it is
    /// not treated as a load on its own. Returns the patch that was loaded.
    pub fn select_slot(&mut self, slot: u8) -> Result<Vec<u8>> {
        let patch = self.read_patch(slot)?;
        self.load_patch(slot, &patch)?;
        Ok(patch)
    }

    /// Load `patch` (a 4096-byte EffectDump) into the edit buffer as the
    /// contents of `slot`, without writing it to memory.
    pub fn load_patch(&mut self, slot: u8, patch: &[u8]) -> Result<()> {
        if patch.len() != protocol::EFFECT_DUMP_LEN {
            return Err(PodError::Protocol(format!(
                "patch data must be exactly {} bytes, got {}",
                protocol::EFFECT_DUMP_LEN,
                patch.len()
            )));
        }
        self.write_raw(&protocol::encode_select_slot(slot))?;
        let (tone1, tone2) = patch.split_at(protocol::TONE_BLOCK_LEN);
        self.push_tone(0, tone1)?;
        self.push_tone(1, tone2)?;
        self.write_raw(&protocol::encode_device_setting(
            protocol::CHANNEL_PATCH,
            protocol::setting::SELECTED_TONE,
            0,
        ))
    }

    /// Read a device-wide setting with a `02`/`21` query; the POD answers
    /// with `04`/`22` carrying the value. IDs `00`-`08` answer (see
    /// [`protocol::setting`]); others get no reply.
    pub fn query_setting(&mut self, id: u32) -> Result<u32> {
        let reply = self.transact(
            &protocol::encode_query(id),
            protocol::MessageType::IntParam12 as u8,
            protocol::QUERY_REPLY_SUB,
            protocol::QUERY_REPLY_LEN,
        )?;
        let got_id = u32::from_le_bytes(reply[12..16].try_into().expect("20-byte reply"));
        if got_id != id {
            return Err(PodError::Protocol(format!(
                "query {id:#x} answered for id {got_id:#x}"
            )));
        }
        Ok(u32::from_le_bytes(
            reply[16..20].try_into().expect("20-byte reply"),
        ))
    }

    /// Set parameter `id` (Gearbox's 32-bit ID: `<namespace u16><idx u16>`)
    /// of the block currently at `slot`/`group` on `tone`.
    pub fn set_param_at(
        &mut self,
        tone: u8,
        slot: u16,
        group: u16,
        id: u32,
        value: f32,
    ) -> Result<()> {
        let msg =
            protocol::encode_float_set(tone, slot, group, id as u16, (id >> 16) as u16, value);
        self.write_raw(&msg)
    }

    /// Turn the block currently at `slot`/`group` on or off.
    pub fn set_enabled_at(&mut self, tone: u8, slot: u16, group: u16, enabled: bool) -> Result<()> {
        let msg = protocol::encode_int_set(
            tone,
            protocol::int_set::BLOCK_ENABLED,
            slot,
            group,
            enabled as u32,
        );
        self.write_raw(&msg)
    }

    /// Move the block at `slot`/`group` to `new_slot`/`new_group` (pre/post
    /// amp). The POD answers by re-announcing the block as enabled.
    pub fn move_block(
        &mut self,
        tone: u8,
        slot: u16,
        group: u16,
        new_slot: u16,
        new_group: u16,
    ) -> Result<()> {
        let msg = protocol::encode_block_move(tone, slot, group, new_slot, new_group);
        self.write_raw(&msg)
    }

    /// Set a tone-level setting (`05`/`16`).
    pub fn set_tone_setting(
        &mut self,
        tone: u8,
        param: u32,
        value: protocol::ToneValue,
    ) -> Result<()> {
        self.write_raw(&protocol::encode_tone_setting(tone, param, value))
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
