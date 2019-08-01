//! PMU Extension
#![allow(missing_docs)]
use e310x::{BACKUP, PMU, RTC};

/// value required written to pmukey register before writing to other PMU registers
pub const PMU_KEY_VAL: u32 = 0x51F15E;

// Hifive1-revA programs
#[cfg(not(feature = "g002"))]
const DEFAULT_SLEEP_PROGRAM: [u32; 8] = [
    0x0F0, // assert corerst
    0x1F0, // assert hfclkrst
    0x1D0, // deassert pmu_out_1
    0x1C0, // deassert pmu_out_0
    0x1C0, // repeats
    0x1C0, 0x1C0, 0x1C0,
];

#[cfg(not(feature = "g002"))]
const DEFAULT_WAKE_PROGRAM: [u32; 8] = [
    0x1F0, // assert all resets and enable all power supplies
    0x0F8, // idle 2^8 cycles, then deassert hfclkrst
    0x030, // deassert corerst and padrst
    0x030, // repeats
    0x030, 0x030, 0x030, 0x030,
];

// Hifive1-revB programs
#[cfg(feature = "g002")]
const DEFAULT_SLEEP_PROGRAM: [u32; 8] = [
    0x2F0, // assert corerst
    0x3F0, // assert hfclkrst
    0x3D0, // deassert pmu_out_1
    0x3C0, // deassert pmu_out_0
    0x3C0, // repeats
    0x3C0, 0x3C0, 0x3C0,
];

#[cfg(feature = "g002")]
const DEFAULT_WAKE_PROGRAM: [u32; 8] = [
    0x3F0, // assert all resets and enable all power supplies
    0x2F8, // idle 2^8 cycles, then deassert hfclkrst
    0x030, // deassert corerst and padrst
    0x030, // repeats
    0x030, 0x030, 0x030, 0x030,
];

pub trait PMUExt {
    fn configure(&self) -> PMUCfg;
}

impl PMUExt for PMU {
    fn configure(&self) -> PMUCfg {
        PMUCfg
    }
}

pub struct PMUCfg;

#[derive(Debug)]
pub enum BackupError {
    DataTooLarge,
    DataMisaligned,
}

impl PMUCfg {
    ///
    /// Resets SLEEP and WAKE programs on the PMU to defaults
    ///
    pub fn load_default_programs(&self) {
        unsafe {
            let pmu = PMU::ptr();

            for i in 0..8 {
                (*pmu).pmukey.write(|w| w.bits(PMU_KEY_VAL));
                (*pmu).pmusleeppm[i].write(|w| w.bits(DEFAULT_SLEEP_PROGRAM[i]));

                (*pmu).pmukey.write(|w| w.bits(PMU_KEY_VAL));
                (*pmu).pmuwakepm[i].write(|w| w.bits(DEFAULT_WAKE_PROGRAM[i]));
            }
        }
    }

    ///
    /// Puts device to sleep for N seconds, allowing wake-up button to wake it up as well
    ///
    /// *sleep_time* - the amount of time to sleep for in seconds
    ///
    /// *Performs following changes to RTC*
    /// - enables RTC clock to be always on
    /// - sets scale to 1/s
    ///
    pub fn sleep(self, sleep_time: u32) {
        unsafe {
            let pmu = PMU::ptr();
            let rtc = RTC::ptr();

            // set interrupt source to RTC enabled, each pmu register needs key set before write
            (*pmu).pmukey.write(|w| w.bits(PMU_KEY_VAL));
            (*pmu)
                .pmuie
                .write(|w| w.rtc().set_bit().dwakeup().set_bit());
            // set RTC clock scale to once per second for easy calculation
            (*rtc)
                .rtccfg
                .write(|w| w.enalways().set_bit().scale().bits(15));
            // get current RTC clock value scaled
            let rtc_now = (*rtc).rtcs.read().bits();
            // set RTC clock comparator
            (*rtc).rtccmp.write(|w| w.bits(rtc_now + sleep_time));
            // go to sleep for sleep_time seconds, need to set pmukey here as well
            (*pmu).pmukey.write(|w| w.bits(PMU_KEY_VAL));
            (*pmu).pmusleep.write(|w| w.sleep().set_bit());
        }
    }

    ///
    /// Stores user data `UD` to backup registers.
    ///
    /// # Arguments
    ///
    /// * `user_data` - the user data to store. *MUST* have alignment of at least 4 and fit into the backup registerss
    ///
    /// # Returns
    /// 
    /// * `Result<UD, BackupError>` - the stored `user_data` is returned on success
    /// 
    /// # Errors
    ///
    /// * `BackupError::DataTooLarge` - returned if `user_data` cannot fit into backup registers
    /// * `BackupError::DataMisaligned` - returned if `user_data` is not aligned to at least 4 bytes
    ///
    /// # Notes
    ///
    /// You can use `#[repr(align(4))]` to enforce a minimum alignment of 4 bytes for `user_data`
    ///
    pub fn store_backup<UD>(&self, user_data: UD) -> Result<UD, BackupError>
    where
        UD: Sized,
    {
        unsafe {
            let backup = BACKUP::ptr();
            let ud_size = core::mem::size_of::<UD>();

            if ud_size > (*backup).backup.len() {
                return Err(BackupError::DataTooLarge);
            }

            if ud_size % 4 != 0 {
                return Err(BackupError::DataMisaligned);
            }

            let ptr = &user_data as *const _;
            let ptr_u32 = ptr as *const u32;
            let sliced = core::slice::from_raw_parts(ptr_u32, ud_size);

            for i in 0..sliced.len() {
                (*backup).backup[i].write(|w| w.bits(sliced[i]));
            }

            Ok(user_data)
        }
    }

    ///
    /// Stores user data `UD` to backup registers.
    ///
    /// # Arguments
    ///
    /// * `user_data` - the user data to restore to. *MUST* have alignment of at least 4 and fit into the backup registerss
    ///
    /// # Returns
    /// 
    /// * `Result<UD, BackupError>` - the restored `user_data` is returned on success
    /// 
    /// # Errors
    ///
    /// * `BackupError::DataTooLarge` - returned if `user_data` cannot fit into backup registers
    /// * `BackupError::DataMisaligned` - returned if `user_data` is not aligned to at least 4 bytes
    ///
    /// # Notes
    ///
    /// You can use `#[repr(align(4))]` to enforce a minimum alignment of 4 bytes for `user_data`
    ///
    pub fn restore_backup<UD>(&self, user_data: UD) -> Result<UD, BackupError>
    where
        UD: Sized,
    {
        unsafe {
            let backup = BACKUP::ptr();
            let ud_size = core::mem::size_of::<UD>();

            if ud_size > (*backup).backup.len() {
                return Err(BackupError::DataTooLarge);
            }

            if ud_size % 4 != 0 {
                return Err(BackupError::DataMisaligned);
            }

            let reg_count = ud_size / 4;

            let ptr = &user_data as *const _;
            let ptr_u32 = ptr as *mut u32;
            let sliced = core::slice::from_raw_parts_mut(ptr_u32, reg_count);

            for i in 0..sliced.len() {
                sliced[i] = (*backup).backup[i].read().bits();
            }

            Ok(user_data)
        }
    }

    ///
    /// Clears all backup registers by setting each to zero
    ///
    pub fn clear_backup(self) {
        unsafe {
            let backup = BACKUP::ptr();

            for i in 0..(*backup).backup.len() {
                (*backup).backup[i].write(|w| w.bits(0u32));
            }
        }
    }
}