use std::ffi::CString;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct DiskSpace {
    pub total: u64,
    pub available: u64,
}

impl DiskSpace {
    pub fn of(path: &Path) -> Option<Self> {
        let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat = MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: `c_path` is a valid NUL-terminated string and `stat` is only read on success.
        let stat = unsafe {
            if libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) != 0 {
                return None;
            }
            stat.assume_init()
        };
        let block = stat.f_frsize;
        Some(Self {
            total: u64::from(stat.f_blocks) * block,
            available: u64::from(stat.f_bavail) * block,
        })
    }

    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}
