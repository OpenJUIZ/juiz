//! 共有メモリのリング (方式 B、docs/spec/large-data.md §6)
//!
//! 送り手 (Topic に publish する juiz) が Topic ごとに 1 つのリングを持ち、本体を枠に書く。
//! IPC の接続では枠の場所だけを送り、受け手は領域を読み取り専用で対応付けて自分の画像へ 1 回コピーする。
//!
//! ```text
//! offset 0           : 領域ヘッダ (4096 byte)
//! offset 4096        : 枠ヘッダ × slots (各 64 byte)
//! offset data_offset : 枠 0 のデータ (slot_bytes)、枠 1 のデータ、...
//! ```
//!
//! 枠ヘッダの seq はシーケンスロック (§6.3)。奇数は書き込み中、偶数は書き終わり。
//! 送り手は受け手を待たずに上書きし、受け手はコピーの前後で seq を確かめて、上書きされていたら捨てる。
//!
//! 領域は OS ごとに名前が残らない作り方をする (§6.6)。
//! Linux は memfd、macOS などは shm_open の直後に shm_unlink、どちらも記述子を SCM_RIGHTS で渡す。
//! Windows はページングファイルの名前付きマッピングで、受け手は名前で開く。

use std::sync::atomic::{fence, AtomicU64, Ordering};
use std::sync::Mutex;

use juiz_sdk::anyhow::anyhow;

use crate::brokers::frame::ShmRef;
use crate::prelude::*;

pub const REGION_MAGIC: [u8; 4] = *b"JZSM";
pub const REGION_VERSION: u32 = 1;
pub const REGION_HEADER_BYTES: u64 = 4096;
pub const SLOT_HEADER_BYTES: u64 = 64;
pub const PAGE: u64 = 4096;

// 領域ヘッダの中の位置
const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 4;
const OFF_SLOTS: usize = 8;
const OFF_SLOT_BYTES: usize = 16;
const OFF_DATA_OFFSET: usize = 24;
const OFF_REGION_ID: usize = 32;

fn round_up(n: u64, unit: u64) -> u64 {
    n.div_ceil(unit) * unit
}

/// リングの形 (枠の数と大きさ、データの位置)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingLayout {
    pub slots: u32,
    pub slot_bytes: u64,
    pub data_offset: u64,
}

impl RingLayout {
    pub fn new(slots: u32, slot_bytes: u64) -> JuizResult<Self> {
        if slots < 2 {
            return Err(anyhow!(JuizError::InvalidValueError {
                message: format!("shared_memory.slots must be 2 or more ({slots})")
            }));
        }
        if slot_bytes == 0 || slot_bytes % PAGE != 0 {
            return Err(anyhow!(JuizError::InvalidValueError {
                message: format!("shared_memory.slot_bytes must be a positive multiple of 4096 ({slot_bytes})")
            }));
        }
        let data_offset = round_up(REGION_HEADER_BYTES + SLOT_HEADER_BYTES * slots as u64, PAGE);
        let layout = RingLayout {
            slots,
            slot_bytes,
            data_offset,
        };
        layout.total_bytes()?;
        Ok(layout)
    }

    pub fn total_bytes(&self) -> JuizResult<u64> {
        (self.slots as u64)
            .checked_mul(self.slot_bytes)
            .and_then(|d| d.checked_add(self.data_offset))
            .filter(|t| usize::try_from(*t).is_ok())
            .ok_or_else(|| anyhow!("shared memory region is too large"))
    }

    fn slot_header(&self, slot: u32) -> usize {
        (REGION_HEADER_BYTES + SLOT_HEADER_BYTES * slot as u64) as usize
    }

    fn slot_data(&self, slot: u32) -> usize {
        (self.data_offset + self.slot_bytes * slot as u64) as usize
    }
}

// ---- OS ごとの領域 ----

#[cfg(unix)]
mod os {
    use std::ffi::CString;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

    use juiz_sdk::anyhow::anyhow;

    use crate::prelude::*;

    /// 対応付けた領域。drop で外す
    pub struct Mapping {
        pub ptr: *mut u8,
        pub len: usize,
        /// 送り手が持つ記述子 (受け手へ渡すため)。受け手は対応付けた後に閉じる
        pub fd: Option<OwnedFd>,
    }

    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                libc::munmap(self.ptr as *mut libc::c_void, self.len);
            }
        }
    }

    fn last_error(what: &str) -> juiz_sdk::anyhow::Error {
        anyhow!("{what} failed: {}", io::Error::last_os_error())
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn create_fd(_region_id: &uuid::Uuid) -> JuizResult<OwnedFd> {
        let name = CString::new("juiz-shm").unwrap();
        let fd = unsafe {
            libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING)
        };
        if fd < 0 {
            return Err(last_error("memfd_create"));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// macOS など: 短い乱数の名前で作り、大きさを決めたらすぐ名前を消す (§6.6)
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn create_fd(region_id: &uuid::Uuid) -> JuizResult<OwnedFd> {
        // macOS の名前は 31 バイトまで
        let simple = region_id.simple().to_string();
        let name = format!("/jz.{}.{}", std::process::id() % 100000, &simple[..16]);
        let cname = CString::new(name).unwrap();
        let fd = unsafe {
            libc::shm_open(
                cname.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(last_error("shm_open"));
        }
        unsafe {
            libc::shm_unlink(cname.as_ptr());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        unsafe {
            let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFD);
            libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
        Ok(fd)
    }

    fn map(fd: RawFd, len: usize, writable: bool) -> JuizResult<*mut u8> {
        let prot = if writable {
            libc::PROT_READ | libc::PROT_WRITE
        } else {
            libc::PROT_READ
        };
        // 最初に触れたときのページフォールト (4K の 1 枠で 10ms 以上) を毎フレームで払わないように、
        // Linux では対応付けるときにページを用意しておく
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let flags = libc::MAP_SHARED | libc::MAP_POPULATE;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let flags = libc::MAP_SHARED;
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, prot, flags, fd, 0) };
        if ptr == libc::MAP_FAILED {
            return Err(last_error("mmap"));
        }
        Ok(ptr as *mut u8)
    }

    /// 送り手: 読み書きできる領域を作る
    pub fn create(len: usize, region_id: &uuid::Uuid) -> JuizResult<Mapping> {
        let fd = create_fd(region_id)?;
        if unsafe { libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) } != 0 {
            return Err(last_error("ftruncate"));
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        unsafe {
            // 受け手が大きさを変えられないように封じる
            libc::fcntl(
                fd.as_raw_fd(),
                libc::F_ADD_SEALS,
                libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
            );
        }
        let ptr = map(fd.as_raw_fd(), len, true)?;
        Ok(Mapping {
            ptr,
            len,
            fd: Some(fd),
        })
    }

    /// 受け手: 受け取った記述子を読み取り専用で対応付ける。記述子は閉じる
    pub fn open_fd(fd: OwnedFd, len: usize) -> JuizResult<Mapping> {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 {
            return Err(last_error("fstat"));
        }
        if (st.st_size as u64) < len as u64 {
            return Err(anyhow!(
                "shared memory region is smaller ({} bytes) than its layout ({len} bytes)",
                st.st_size
            ));
        }
        let ptr = map(fd.as_raw_fd(), len, false)?;
        Ok(Mapping { ptr, len, fd: None })
    }
}

#[cfg(windows)]
mod os {
    use std::io;

    use juiz_sdk::anyhow::anyhow;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
        FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
    };

    use crate::prelude::*;

    /// 作ったユーザー (OWNER RIGHTS) と SYSTEM だけに許す (IPC の名前付きパイプと同じ)
    const SHM_SDDL: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

    pub struct Mapping {
        pub ptr: *mut u8,
        pub len: usize,
        handle: HANDLE,
    }

    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.ptr as *mut _,
                });
                CloseHandle(self.handle);
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn mapping_name(region_id: &str) -> String {
        format!("Local\\juiz-shm-{region_id}")
    }

    fn last_error(what: &str) -> juiz_sdk::anyhow::Error {
        anyhow!("{what} failed: {}", io::Error::last_os_error())
    }

    fn view(handle: HANDLE, access: u32, len: usize) -> JuizResult<*mut u8> {
        let v = unsafe { MapViewOfFile(handle, access, 0, 0, len) };
        if v.Value.is_null() {
            let e = last_error("MapViewOfFile");
            unsafe {
                CloseHandle(handle);
            }
            return Err(e);
        }
        Ok(v.Value as *mut u8)
    }

    pub fn create(len: usize, region_id: &uuid::Uuid) -> JuizResult<Mapping> {
        let name = wide(&mapping_name(&region_id.to_string()));
        let sddl = wide(SHM_SDDL);
        let mut sd = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_error("ConvertStringSecurityDescriptorToSecurityDescriptorW"));
        }
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let size = len as u64;
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                &sa,
                PAGE_READWRITE,
                (size >> 32) as u32,
                size as u32,
                name.as_ptr(),
            )
        };
        unsafe {
            LocalFree(sd as _);
        }
        if handle.is_null() {
            return Err(last_error("CreateFileMappingW"));
        }
        let ptr = view(handle, FILE_MAP_READ | FILE_MAP_WRITE, len)?;
        Ok(Mapping { ptr, len, handle })
    }

    pub fn open_name(region_id: &str, len: usize) -> JuizResult<Mapping> {
        let name = wide(&mapping_name(region_id));
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(last_error("OpenFileMappingW"));
        }
        let ptr = view(handle, FILE_MAP_READ, len)?;
        Ok(Mapping { ptr, len, handle })
    }
}

#[cfg(unix)]
pub use std::os::fd::OwnedFd;

// ---- 送り手 ----

/// 送り手のリング。Topic ごとに 1 つ
pub struct ShmRing {
    region_id: uuid::Uuid,
    layout: RingLayout,
    mapping: os::Mapping,
    /// 最後に書いた通し番号 n。書き込みは 1 つずつ
    next: Mutex<u64>,
}

impl std::fmt::Debug for ShmRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmRing")
            .field("region_id", &self.region_id)
            .field("layout", &self.layout)
            .finish()
    }
}

impl ShmRing {
    pub fn create(layout: RingLayout) -> JuizResult<Self> {
        let region_id = uuid::Uuid::new_v4();
        let len = layout.total_bytes()? as usize;
        let mapping = os::create(len, &region_id)?;
        // Linux 以外では、ページを 1 度ずつ触って先に用意しておく (Linux は MAP_POPULATE)
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        unsafe {
            let mut off = 0;
            while off < len {
                std::ptr::write_volatile(mapping.ptr.add(off), 0);
                off += PAGE as usize;
            }
        }
        // 領域ヘッダを書く (枠ヘッダは 0 = まだ書いていない)
        unsafe {
            let base = mapping.ptr;
            std::ptr::copy_nonoverlapping(REGION_MAGIC.as_ptr(), base.add(OFF_MAGIC), 4);
            std::ptr::write_unaligned(base.add(OFF_VERSION) as *mut u32, REGION_VERSION.to_le());
            std::ptr::write_unaligned(base.add(OFF_SLOTS) as *mut u32, layout.slots.to_le());
            std::ptr::write_unaligned(base.add(OFF_SLOT_BYTES) as *mut u64, layout.slot_bytes.to_le());
            std::ptr::write_unaligned(base.add(OFF_DATA_OFFSET) as *mut u64, layout.data_offset.to_le());
            std::ptr::copy_nonoverlapping(region_id.as_bytes().as_ptr(), base.add(OFF_REGION_ID), 16);
        }
        Ok(ShmRing {
            region_id,
            layout,
            mapping,
            next: Mutex::new(0),
        })
    }

    pub fn region_id(&self) -> String {
        self.region_id.to_string()
    }

    pub fn layout(&self) -> RingLayout {
        self.layout
    }

    /// 受け手へ渡す記述子
    #[cfg(unix)]
    pub fn fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd;
        self.mapping.fd.as_ref().map(|f| f.as_raw_fd()).unwrap_or(-1)
    }

    fn seq_of(&self, slot: u32) -> &AtomicU64 {
        // SAFETY: 枠ヘッダは 64 byte ごとに並び、先頭は 8 byte に揃っている。領域は self が生きている間ある
        unsafe { &*(self.mapping.ptr.add(self.layout.slot_header(slot)) as *const AtomicU64) }
    }

    /// 本体を次の枠に書く (§6.3)。len が枠より大きければエラー。fill は len バイトの領域に本体を書く
    pub fn write(&self, len: u64, fill: impl FnOnce(&mut [u8]) -> JuizResult<()>) -> JuizResult<ShmRef> {
        if len > self.layout.slot_bytes {
            return Err(anyhow!(
                "body ({len} bytes) is larger than a slot ({} bytes)",
                self.layout.slot_bytes
            ));
        }
        let mut next = self
            .next
            .lock()
            .map_err(|_| anyhow!("ShmRing lock is poisoned"))?;
        let n = *next + 1;
        let slot = (n % self.layout.slots as u64) as u32;
        let seq = self.seq_of(slot);
        seq.store(2 * n - 1, Ordering::Relaxed);
        fence(Ordering::Release);
        // SAFETY: 枠のデータは領域の中にあり (RingLayout::new で確かめた)、書くのはこのスレッドだけ
        let dst = unsafe {
            std::slice::from_raw_parts_mut(
                self.mapping.ptr.add(self.layout.slot_data(slot)),
                len as usize,
            )
        };
        let r = fill(dst);
        unsafe {
            std::ptr::write_unaligned(
                self.mapping.ptr.add(self.layout.slot_header(slot) + 8) as *mut u64,
                len.to_le(),
            );
        }
        // 失敗しても枠は書き終わりにしておく (中身は使われない。通し番号は進める)
        seq.store(2 * n, Ordering::Release);
        *next = n;
        r?;
        Ok(ShmRef {
            region: self.region_id(),
            slot,
            seq: 2 * n,
            len,
        })
    }
}

// ---- 受け手 ----

/// 本体を読めなかった理由
#[derive(Debug)]
pub enum ShmReadError {
    /// 読む前か読んでいる最中に上書きされた (§6.5、エラーにせず捨てて数える)
    Overwritten,
    /// 通知が領域の外を指している
    Invalid(juiz_sdk::anyhow::Error),
}

/// 受け手が対応付けた領域 (読み取り専用)
pub struct ShmReader {
    region_id: String,
    layout: RingLayout,
    mapping: os::Mapping,
}

impl std::fmt::Debug for ShmReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmReader")
            .field("region_id", &self.region_id)
            .field("layout", &self.layout)
            .finish()
    }
}

impl ShmReader {
    #[cfg(unix)]
    pub fn open(fd: OwnedFd, region_id: &str, layout: RingLayout) -> JuizResult<Self> {
        let mapping = os::open_fd(fd, layout.total_bytes()? as usize)?;
        Self::check(region_id, layout, mapping)
    }

    #[cfg(windows)]
    pub fn open(region_id: &str, layout: RingLayout) -> JuizResult<Self> {
        uuid::Uuid::parse_str(region_id)?;
        let mapping = os::open_name(region_id, layout.total_bytes()? as usize)?;
        Self::check(region_id, layout, mapping)
    }

    /// 領域ヘッダが attach の知らせと合うかを確かめる
    fn check(region_id: &str, layout: RingLayout, mapping: os::Mapping) -> JuizResult<Self> {
        let base = mapping.ptr;
        let (magic, version, slots, slot_bytes, data_offset, id) = unsafe {
            let mut magic = [0u8; 4];
            std::ptr::copy_nonoverlapping(base.add(OFF_MAGIC), magic.as_mut_ptr(), 4);
            let mut id = [0u8; 16];
            std::ptr::copy_nonoverlapping(base.add(OFF_REGION_ID), id.as_mut_ptr(), 16);
            (
                magic,
                u32::from_le(std::ptr::read_unaligned(base.add(OFF_VERSION) as *const u32)),
                u32::from_le(std::ptr::read_unaligned(base.add(OFF_SLOTS) as *const u32)),
                u64::from_le(std::ptr::read_unaligned(base.add(OFF_SLOT_BYTES) as *const u64)),
                u64::from_le(std::ptr::read_unaligned(base.add(OFF_DATA_OFFSET) as *const u64)),
                uuid::Uuid::from_bytes(id),
            )
        };
        if magic != REGION_MAGIC || version != REGION_VERSION {
            return Err(anyhow!("this is not a juiz shared memory region (magic {magic:?}, version {version})"));
        }
        if slots != layout.slots
            || slot_bytes != layout.slot_bytes
            || data_offset != layout.data_offset
            || id.to_string() != region_id
        {
            return Err(anyhow!(
                "shared memory region header does not match the attach request ({slots} slots, {slot_bytes} bytes, data at {data_offset}, id {id})"
            ));
        }
        Ok(ShmReader {
            region_id: region_id.to_owned(),
            layout,
            mapping,
        })
    }

    pub fn region_id(&self) -> &str {
        &self.region_id
    }

    pub fn layout(&self) -> RingLayout {
        self.layout
    }

    fn seq_of(&self, slot: u32) -> &AtomicU64 {
        // SAFETY: ShmRing::seq_of と同じ。読むだけ
        unsafe { &*(self.mapping.ptr.add(self.layout.slot_header(slot)) as *const AtomicU64) }
    }

    /// 通知の枠を、シーケンスロックで確かめながら読む (§6.4)。read は枠のデータ (len バイト) を受け取り、
    /// 自分の領域へコピーした結果を返す。コピーの後に上書きがわかれば、その結果は捨てる
    pub fn read<T>(
        &self,
        r: &ShmRef,
        read: impl FnOnce(&[u8]) -> JuizResult<T>,
    ) -> Result<T, ShmReadError> {
        if r.region != self.region_id {
            return Err(ShmReadError::Invalid(anyhow!("unknown region {}", r.region)));
        }
        if r.slot >= self.layout.slots || r.len > self.layout.slot_bytes || r.seq == 0 || r.seq % 2 != 0 {
            return Err(ShmReadError::Invalid(anyhow!(
                "shared memory reference is out of the region (slot {}, len {}, seq {})",
                r.slot,
                r.len,
                r.seq
            )));
        }
        let seq = self.seq_of(r.slot);
        if seq.load(Ordering::Acquire) != r.seq {
            return Err(ShmReadError::Overwritten);
        }
        // SAFETY: 範囲は上で確かめた。送り手が同時に書き換えることはあるが、その結果は下の seq で捨てる
        let src = unsafe {
            std::slice::from_raw_parts(
                self.mapping.ptr.add(self.layout.slot_data(r.slot)),
                r.len as usize,
            )
        };
        let value = read(src).map_err(ShmReadError::Invalid)?;
        fence(Ordering::Acquire);
        if seq.load(Ordering::Relaxed) != r.seq {
            return Err(ShmReadError::Overwritten);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader_for(ring: &ShmRing) -> ShmReader {
        #[cfg(unix)]
        {
            let fd = unsafe { libc::dup(ring.fd()) };
            assert!(fd >= 0);
            let fd = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
            ShmReader::open(fd, &ring.region_id(), ring.layout()).unwrap()
        }
        #[cfg(windows)]
        {
            ShmReader::open(&ring.region_id(), ring.layout()).unwrap()
        }
    }

    #[test]
    fn layout_is_page_aligned() {
        let l = RingLayout::new(4, 1 << 20).unwrap();
        assert_eq!(l.data_offset, 8192);
        assert_eq!(l.total_bytes().unwrap(), 8192 + 4 * (1 << 20));
        assert!(RingLayout::new(1, 4096).is_err());
        assert!(RingLayout::new(4, 1000).is_err());
    }

    #[test]
    fn write_and_read_slots() {
        let ring = ShmRing::create(RingLayout::new(2, 4096).unwrap()).unwrap();
        let reader = reader_for(&ring);
        let r1 = ring.write(3, |d| Ok(d.copy_from_slice(&[1, 2, 3]))).unwrap();
        assert_eq!((r1.slot, r1.seq, r1.len), (1, 2, 3));
        assert_eq!(reader.read(&r1, |s| Ok(s.to_vec())).unwrap(), vec![1, 2, 3]);
        let r2 = ring.write(2, |d| Ok(d.copy_from_slice(&[4, 5]))).unwrap();
        assert_eq!((r2.slot, r2.seq), (0, 4));
        // 枠 1 を上書きすると、r1 は読めない
        let r3 = ring.write(1, |d| Ok(d.copy_from_slice(&[6]))).unwrap();
        assert_eq!(r3.slot, 1);
        assert!(matches!(reader.read(&r1, |s| Ok(s.to_vec())), Err(ShmReadError::Overwritten)));
        assert_eq!(reader.read(&r2, |s| Ok(s.to_vec())).unwrap(), vec![4, 5]);
        // 読んでいる最中に上書きされたら捨てる
        let r4 = ring.write(1, |d| Ok(d.copy_from_slice(&[7]))).unwrap();
        let r = reader.read(&r4, |s| {
            let v = s.to_vec();
            ring.write(1, |d| Ok(d.copy_from_slice(&[8]))).unwrap();
            ring.write(1, |d| Ok(d.copy_from_slice(&[9]))).unwrap();
            Ok(v)
        });
        assert!(matches!(r, Err(ShmReadError::Overwritten)));
        // 大きすぎる本体と、領域の外を指す通知
        assert!(ring.write(4097, |_| Ok(())).is_err());
        let mut bad = r3.clone();
        bad.slot = 5;
        assert!(matches!(reader.read(&bad, |s| Ok(s.len())), Err(ShmReadError::Invalid(_))));
        bad = r3.clone();
        bad.region = uuid::Uuid::new_v4().to_string();
        assert!(matches!(reader.read(&bad, |s| Ok(s.len())), Err(ShmReadError::Invalid(_))));
    }

    #[test]
    fn reader_rejects_wrong_layout() {
        let ring = ShmRing::create(RingLayout::new(2, 4096).unwrap()).unwrap();
        #[cfg(unix)]
        {
            let fd = unsafe { libc::dup(ring.fd()) };
            let fd = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
            assert!(ShmReader::open(fd, &ring.region_id(), RingLayout::new(3, 4096).unwrap()).is_err());
        }
        #[cfg(windows)]
        assert!(ShmReader::open(&ring.region_id(), RingLayout::new(3, 4096).unwrap()).is_err());
    }
}

