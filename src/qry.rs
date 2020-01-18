use crate::error::{PtError, deref_ptresult, ensure_ptok, extract_pterr};
use crate::Config;
use crate::Status;
use crate::Event;

use std::convert::TryFrom;
use std::marker::PhantomData;
use std::mem;

use num_enum::TryFromPrimitive;
use libipt_sys::{
    pt_qry_alloc_decoder,
    pt_query_decoder,
    pt_qry_cond_branch,
    pt_qry_core_bus_ratio,
    pt_qry_event,
    pt_event,
    pt_qry_free_decoder,
    pt_qry_get_config,
    pt_qry_get_offset,
    pt_qry_get_sync_offset,
    pt_qry_indirect_branch,
    pt_qry_sync_backward,
    pt_qry_sync_forward,
    pt_qry_sync_set,
    pt_qry_time
};

#[derive(Clone, Copy, TryFromPrimitive)]
#[repr(i32)]
pub enum CondBranch {
    Taken = 1,
    NotTaken = 0
}

pub struct QueryDecoder<T>(pt_query_decoder, PhantomData<T>);
impl<T> QueryDecoder<T> {
    /// Allocate an Intel PT query decoder.
    ///
    /// The decoder will work on the buffer defined in @config,
    /// it shall contain raw trace data and remain valid for the lifetime of the decoder.
    /// The decoder needs to be synchronized before it can be used.
    pub fn new(cfg: &Config<T>) -> Result<Self, PtError> {
        deref_ptresult(unsafe { pt_qry_alloc_decoder(&cfg.0) })
            .map(|d| QueryDecoder::<T>(*d, PhantomData))
    }

    /// Query whether the next unconditional branch has been taken.
    ///
    /// On success, provides Taken or NotTaken along with StatusFlags
    /// for the next conditional branch and updates decoder.
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns BadQuery if no conditional branch is found.
    /// Returns Eos if decoding reached the end of the Intel PT buffer.
    /// Returns Nosync if decoder is out of sync.
    pub fn cond_branch(&mut self) -> Result<(CondBranch, Status), PtError> {
        let mut taken: i32 = 0;
        extract_pterr(unsafe { pt_qry_cond_branch(&mut self.0, &mut taken) })
            .map(|s| (
                CondBranch::try_from(taken).unwrap(),
                Status::from_bits(s).unwrap()))
    }

    /// Return the current core bus ratio.
    ///
    /// On success, provides the current core:bus ratio
    /// The ratio is defined as core cycles per bus clock cycle.
    /// Returns NoCbr if there has not been a CBR packet.
    pub fn core_bus_ratio(&mut self) -> Result<u32, PtError> {
        let mut cbr: u32 = 0;
        ensure_ptok(unsafe { pt_qry_core_bus_ratio(&mut self.0, &mut cbr) })
            .map(|_| cbr)
    }

    /// Query the next pending event.
    ///
    /// On success, provides the next event along with its status and updates the decoder.
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns BadQuery if no event is found.
    /// Returns Eos if decoding reached the end of the Intel PT buffer.
    /// Returns Nosync if decoder is out of sync.
    pub fn event(&mut self) -> Result<(Event, Status), PtError> {
        let mut evt: pt_event = unsafe { mem::zeroed() };
        extract_pterr(unsafe {
            pt_qry_event(&mut self.0,
                         &mut evt,
                         mem::size_of::<pt_event>())
        }).map(|s| (Event(evt), Status::from_bits(s).unwrap()))
    }

    pub fn config(&self) -> Result<Config<T>, PtError> {
        deref_ptresult(unsafe { pt_qry_get_config(&self.0) })
            .map(Config::from)
    }

    /// Get the current decoder position.
    ///
    /// Returns Nosync if decoder is out of sync.
    pub fn offset(&self) -> Result<u64, PtError> {
        let mut off: u64 = 0;
        ensure_ptok(unsafe { pt_qry_get_offset(&self.0, &mut off) })
            .map(|_| off)
    }

    /// Get the position of the last synchronization point.
    ///
    /// This is useful for splitting a trace stream for parallel decoding.
    /// Returns Nosync if decoder is out of sync.
    pub fn sync_offset(&self) -> Result<u64, PtError> {
        let mut off: u64 = 0;
        ensure_ptok(unsafe { pt_qry_get_sync_offset(&self.0, &mut off) })
            .map(|_| off)
    }

    /// Get the next indirect branch destination.
    ///
    /// On success, provides the linear destination address
    /// of the next indirect branch along with the status
    /// and updates the decoder.
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns BadQuery if no indirect branch is found.
    /// Returns Eos if decoding reached the end of the Intel PT buffer.
    /// Returns Nosync if decoder is out of sync.
    pub fn indirect_branch(&mut self) -> Result<u64, PtError> {
        let mut ip: u64 = 0;
        ensure_ptok(unsafe { pt_qry_indirect_branch(&mut self.0, &mut ip) })
            .map(|_| ip)
    }

    /// Synchronize an Intel PT query decoder.
    ///
    /// Search for the next synchronization point in forward or backward direction.
    /// If decoder has not been synchronized, yet, the search is started at the beginning
    /// of the trace buffer in case of forward synchronization
    /// and at the end of the trace buffer in case of backward synchronization.
    /// Returns the last ip along with a non-negative Status on success
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns Eos if no further synchronization point is found.
    pub fn sync_backward(&mut self) -> Result<(u64, Status), PtError> {
        let mut ip: u64 = 0;
        extract_pterr(unsafe { pt_qry_sync_backward(&mut self.0, &mut ip)})
            .map(|s| (ip, Status::from_bits(s).unwrap()))
    }

    /// Synchronize an Intel PT query decoder.
    ///
    /// Search for the next synchronization point in forward or backward direction.
    /// If decoder has not been synchronized, yet, the search is started at the beginning
    /// of the trace buffer in case of forward synchronization
    /// and at the end of the trace buffer in case of backward synchronization.
    /// Returns the last ip along with a non-negative Status on success
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns Eos if no further synchronization point is found.
    pub fn sync_forward(&mut self) -> Result<(u64, Status), PtError> {
        let mut ip: u64 = 0;
        extract_pterr(unsafe { pt_qry_sync_forward(&mut self.0, &mut ip) })
            .map(|s| (ip, Status::from_bits(s).unwrap()))
    }

    /// Manually synchronize an Intel PT query decoder.
    ///
    /// Synchronize decoder on the syncpoint at @offset.
    /// There must be a PSB packet at @offset.
    /// Returns last ip along with a status.
    /// Returns BadOpc if an unknown packet is encountered.
    /// Returns BadPacket if an unknown packet payload is encountered.
    /// Returns Eos if @offset lies outside of decoder's trace buffer.
    /// Returns Eos if decoder reaches the end of its trace buffer.
    /// Returns Nosync if there is no syncpoint at @offset.
    pub fn sync_set(&mut self, offset: u64) -> Result<(u64, Status), PtError> {
        let mut ip: u64 = 0;
        extract_pterr(unsafe { pt_qry_sync_set(&mut self.0, &mut ip, offset)})
            .map(|s| (ip, Status::from_bits(s).unwrap()))
    }

    /// Query the current time.
    ///
    /// On success, provides the time at the last query.
    /// The time is similar to what a rdtsc instruction would return.
    /// Depending on the configuration, the time may not be fully accurate.
    /// If TSC is not enabled, the time is relative to the last synchronization
    /// and can't be used to correlate with other TSC-based time sources.
    /// In this case, NoTime is returned and the relative time is provided.
    /// Some timing-related packets may need to be dropped (mostly due to missing calibration or incomplete configuration).
    /// To get an idea about the quality of the estimated time, we record the number of dropped MTC and CYC packets.
    /// Returns time, number of lost mtc packets and number of lost cyc packets.
    /// Returns NoTime if there has not been a TSC packet.
    pub fn time(&mut self) -> Result<(u64, u32, u32), PtError> {
        let mut time: u64 = 0;
        let mut mtc: u32 = 0;
        let mut cyc: u32 = 0;
        ensure_ptok(unsafe {
            pt_qry_time(&mut self.0,
                        &mut time,
                        &mut mtc,
                        &mut cyc)
        }).map(|_| (time, mtc, cyc))
    }
}

impl<T> Drop for QueryDecoder<T> {
    fn drop(&mut self) { unsafe { pt_qry_free_decoder(&mut self.0) }}
}