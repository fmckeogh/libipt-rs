use super::Block;
use crate::asid::Asid;
use crate::config::Config;
use crate::error::{PtError, ensure_ptok, extract_pterr, deref_ptresult};
use crate::event::Event;
use crate::flags::Status;
use crate::image::Image;

use std::mem;
use std::ptr;

use libipt_sys::{
    pt_event,
    pt_block,
    pt_block_decoder,
    pt_blk_alloc_decoder,
    pt_blk_core_bus_ratio,
    pt_blk_free_decoder,
    pt_blk_get_config,
    pt_blk_get_image,
    pt_blk_get_offset,
    pt_blk_get_sync_offset,
    pt_blk_set_image,
    pt_blk_sync_backward,
    pt_blk_sync_forward,
    pt_blk_sync_set,
    pt_blk_time,
    pt_blk_next,
    pt_blk_event,
    pt_blk_asid,
    pt_asid
};

pub struct BlockDecoder(pt_block_decoder);
impl BlockDecoder {
    /// Allocate an Intel PT block decoder.
    ///
    /// The decoder will work on the buffer defined in @config,
    /// it shall contain raw trace data and remain valid for the lifetime of the decoder.
    /// The decoder needs to be synchronized before it can be used.
    pub fn new(cfg: &Config) -> Result<Self, PtError> {
        deref_ptresult(unsafe{ pt_blk_alloc_decoder(&cfg.0) })
            .map(|x| BlockDecoder(*x))
    }

    /// Return the current address space identifier.
    ///
    /// On success, provides the current address space identifier in @asid.
    /// Returns Asid on success, a PtError otherwise.
    pub fn asid(&self) -> Result<Asid, PtError> {
        let mut a = Asid::new(0, 0);
        unsafe {
            ensure_ptok(pt_blk_asid(&self.0, &mut a.0, mem::size_of::<pt_asid>()))
                .map(|_| a)
        }
    }

    /// Return the current core bus ratio.
    ///
    /// On success, provides the current core:bus ratio.
    /// The ratio is defined as core cycles per bus clock cycle.
    /// Returns NoCbr if there has not been a CBR packet.
    pub fn core_bus_ratio(&mut self) -> Result<u32, PtError> {
        let mut cbr: u32 = 0;
        unsafe { extract_pterr(pt_blk_core_bus_ratio(&mut self.0, &mut cbr)) }
    }

    /// Get the next pending event.
    ///
    /// On success, provides the next event, a StatusFlag instance and updates the decoder.
    /// Returns BadQuery if there is no event.
    pub fn event(&mut self) -> Result<(Event, Status), PtError> {
        let mut evt: pt_event = unsafe { mem::zeroed() };
        extract_pterr(unsafe {
            pt_blk_event(&mut self.0,
                         &mut evt,
                         mem::size_of::<pt_event>())
        }).map(|s| (Event(evt), Status::from_bits(s).unwrap()))
    }

    pub fn config(&self) -> Result<Config, PtError> {
        deref_ptresult(unsafe { pt_blk_get_config(&self.0) })
            .map(Config::from)
    }

    /// Get the traced image.
    ///
    /// The returned image may be modified as long as @decoder is not running.
    /// Returns the traced image the decoder uses for reading memory.
    pub fn image(&mut self) -> Result<Image, PtError> {
        deref_ptresult(unsafe { pt_blk_get_image(&mut self.0) })
            .map(|i| Image(*i))
    }

    /// Get the current decoder position.
    ///
    /// Returns the current decoder position.
    /// Returns Nosync if decoder is out of sync.
    pub fn offset(&self) -> Result<u64, PtError> {
        let mut off: u64 = 0;
        ensure_ptok(unsafe { pt_blk_get_offset(&self.0, &mut off) })
            .map(|_| off)
    }

    /// Get the position of the last synchronization point.
    ///
    /// Returns Nosync if the decoder is out of sync.
    pub fn sync_offset(&self) -> Result<u64, PtError> {
        let mut off: u64 = 0;
        ensure_ptok(unsafe { pt_blk_get_sync_offset(&self.0, &mut off) })
            .map(|_| off)
    }

    /// Determine the next block of instructions.
    ///
    /// On success, provides the next block of instructions in execution order.
    /// Also Returns a StatusFlag instance on success.
    /// Returns Eos to indicate the end of the trace stream.
    /// Subsequent calls to next will continue to return Eos until trace is required to determine the next instruction.
    /// Returns BadContext if the decoder encountered an unexpected packet.
    /// Returns BadOpc if the decoder encountered unknown packets.
    /// Returns BadPacket if the decoder encountered unknown packet payloads.
    /// Returns BadQuery if the decoder got out of sync.
    /// Returns Eos if decoding reached the end of the Intel PT buffer.
    /// Returns Nomap if the memory at the instruction address can't be read.
    /// Returns Nosync if the decoder is out of sync.
    pub fn next(&mut self) -> Result<Block, PtError> {
        let mut blk: pt_block = unsafe { mem::zeroed() };
        ensure_ptok(
            unsafe {
                pt_blk_next(&mut self.0,
                            &mut blk,
                            mem::size_of::<pt_block>())
            }
        ).map(|_| Block(blk))
    }

    /// Set the traced image.
    ///
    /// Sets the image that the decoder uses for reading memory to image.
    /// If image is None, sets the image to the decoder's default image.
    /// Only one image can be active at any time.
    pub fn set_image(&mut self, img: Option<&mut Image>) -> Result<(), PtError> {
        ensure_ptok(unsafe {
            pt_blk_set_image(&mut self.0,
                             match img {
                                 None => ptr::null_mut(),
                                 Some(i) => &mut i.0
                             })
        })
    }

    pub fn sync_backward(&mut self) -> Result<(), PtError> {
        ensure_ptok(unsafe { pt_blk_sync_backward(&mut self.0) })
    }

    /// Synchronize an Intel PT block decoder.
    ///
    /// Search for the next synchronization point in forward or backward direction.
    /// If the decoder has not been synchronized, yet,
    /// the search is started at the beginning of the trace buffer in case of forward synchronization and at the end of the trace buffer in case of backward synchronization.
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns Eos if no further synchronization point is found.
    pub fn sync_forward(&mut self) -> Result<(), PtError> {
        ensure_ptok(unsafe { pt_blk_sync_forward(&mut self.0) })
    }

    /// Manually synchronize an Intel PT block decoder.
    ///
    /// Synchronize @decoder on the syncpoint at @offset. There must be a PSB packet at @offset.
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns Eos if @offset lies outside of the decoder's trace buffer.
    /// Returns Eos if the decoder reaches the end of its trace buffer.
    /// Returns Nosync if there is no syncpoint at @offset.
    pub fn set_sync(&mut self, offset: u64) -> Result<(), PtError> {
        ensure_ptok(unsafe { pt_blk_sync_set(&mut self.0, offset)})
    }

    /// Return the current time.
    ///
    /// On success, provides the time at the last preceding timing packet,
    /// The number of lost mtc packets and
    /// The number of lost cyc packets.
    ///
    /// The time is similar to what a rdtsc instruction would return.
    /// Depending on the configuration, the time may not be fully accurate.
    /// If TSC is not enabled, the time is relative to the last synchronization and can't be used to correlate with other TSC-based time sources.
    /// In this case, NoTime is returned and the relative time is provided.
    /// Some timing-related packets may need to be dropped (mostly due to missing calibration or incomplete configuration).
    /// To get an idea about the quality of the estimated time, we record the number of dropped MTC and CYC packets.
    /// Returns NoTime if there has not been a TSC packet.
    pub fn time(&mut self) -> Result<(u64, u32, u32), PtError> {
        let mut time: u64 = 0;
        let mut lost_mtc: u32 = 0;
        let mut lost_cyc: u32 = 0;
        ensure_ptok(
            unsafe {
                pt_blk_time(&mut self.0,
                            &mut time,
                            &mut lost_mtc,
                            &mut lost_cyc)
            }
        ).map(|_| (time, lost_mtc, lost_cyc))
    }
}

impl Drop for BlockDecoder {
    fn drop(&mut self) { unsafe { pt_blk_free_decoder(&mut self.0) } }
}