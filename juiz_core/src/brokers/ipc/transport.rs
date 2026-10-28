//! IPC の通り道。Unix は Unix ドメインソケット、Windows は名前付きパイプ (docs/spec/ipc-broker.md §2)

#[cfg(unix)]
mod imp {
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::time::Duration;

    use juiz_sdk::anyhow::anyhow;

    use super::super::ipc_name::ipc_socket_path;
    use crate::prelude::*;

    pub type Stream = UnixStream;

    pub struct Listener {
        inner: UnixListener,
        path: PathBuf,
    }

    pub fn connect(name: &str) -> JuizResult<Stream> {
        let path = ipc_socket_path(name)?;
        UnixStream::connect(&path)
            .map_err(|e| anyhow!("can not connect to ipc://{name} ({}): {e}", path.display()))
    }

    /// 待ち受けを始める。前のサーバーが残したソケットのファイルは、繋がらなければ消して作り直す
    pub fn bind(name: &str) -> JuizResult<Listener> {
        let path = ipc_socket_path(name)?;
        if std::fs::symlink_metadata(&path).is_ok() {
            match UnixStream::connect(&path) {
                Ok(_) => {
                    return Err(anyhow!(JuizError::InvalidValueError {
                        message: format!("ipc://{name} is already in use ({})", path.display())
                    }))
                }
                Err(e) => {
                    log::info!(
                        "removing a stale ipc socket {} ({e})",
                        path.display()
                    );
                    std::fs::remove_file(&path).map_err(|e| {
                        anyhow!("can not remove stale ipc socket {}: {e}", path.display())
                    })?;
                }
            }
        }
        let inner = UnixListener::bind(&path)
            .map_err(|e| anyhow!("can not listen on ipc://{name} ({}): {e}", path.display()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Listener { inner, path })
    }

    impl Listener {
        pub fn accept(&self) -> io::Result<Stream> {
            self.inner.accept().map(|(s, _)| s)
        }

        /// accept で待っているスレッドを起こす
        pub fn wake(&self) {
            let _ = UnixStream::connect(&self.path);
        }

        pub fn location(&self) -> String {
            self.path.display().to_string()
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub fn set_timeout(stream: &Stream, timeout: Option<Duration>) -> io::Result<()> {
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)
    }

    pub fn try_clone(stream: &Stream) -> io::Result<Stream> {
        stream.try_clone()
    }

    pub fn shutdown(stream: &Stream) {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }

    /// 受け手の読み取り。届いたファイル記述子 (SCM_RIGHTS、共有メモリ §6.6) を取っておく
    pub struct RecvStream {
        inner: UnixStream,
        fds: std::collections::VecDeque<std::os::fd::OwnedFd>,
    }

    impl RecvStream {
        pub fn new(inner: UnixStream) -> Self {
            RecvStream {
                inner,
                fds: Default::default(),
            }
        }

        /// 届いた順に記述子を 1 つ取り出す
        pub fn take_fd(&mut self) -> Option<std::os::fd::OwnedFd> {
            self.fds.pop_front()
        }
    }

    /// 1 回に受け取る記述子の数の上限
    const MAX_FDS: usize = 4;

    impl io::Read for RecvStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
            let space =
                unsafe { libc::CMSG_SPACE((MAX_FDS * std::mem::size_of::<libc::c_int>()) as u32) }
                    as usize;
            // cmsghdr の位置合わせのため u64 で確保する
            let mut cbuf = vec![0u64; space.div_ceil(8)];
            let mut iov = libc::iovec {
                iov_base: buf.as_mut_ptr() as *mut libc::c_void,
                iov_len: buf.len(),
            };
            let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = cbuf.as_mut_ptr() as *mut libc::c_void;
            msg.msg_controllen = space as _;
            #[cfg(any(target_os = "linux", target_os = "android"))]
            let flags = libc::MSG_CMSG_CLOEXEC;
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            let flags = 0;
            let n = loop {
                let n = unsafe { libc::recvmsg(self.inner.as_raw_fd(), &mut msg, flags) };
                if n < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                }
                break n as usize;
            };
            unsafe {
                let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
                while !cmsg.is_null() {
                    if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                        let data = libc::CMSG_DATA(cmsg) as *const libc::c_int;
                        let header = libc::CMSG_LEN(0) as usize;
                        let count = ((*cmsg).cmsg_len as usize - header) / std::mem::size_of::<libc::c_int>();
                        for i in 0..count {
                            let fd = std::ptr::read_unaligned(data.add(i));
                            #[cfg(not(any(target_os = "linux", target_os = "android")))]
                            {
                                let f = libc::fcntl(fd, libc::F_GETFD);
                                libc::fcntl(fd, libc::F_SETFD, f | libc::FD_CLOEXEC);
                            }
                            self.fds.push_back(OwnedFd::from_raw_fd(fd));
                        }
                    }
                    cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
                }
            }
            if msg.msg_flags & libc::MSG_CTRUNC != 0 {
                log::warn!("ipc: some file descriptors were dropped (too many in one message)");
            }
            Ok(n)
        }
    }

    /// data を記述子 fd と一緒に送る。記述子はデータの先頭に付く
    pub fn send_with_fd(stream: &Stream, data: &[u8], fd: std::os::fd::RawFd) -> io::Result<()> {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        if data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no data to send with a file descriptor"));
        }
        let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as u32) } as usize;
        let mut cbuf = vec![0u64; space.div_ceil(8)];
        let mut iov = libc::iovec {
            iov_base: data.as_ptr() as *mut libc::c_void,
            iov_len: data.len(),
        };
        let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cbuf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = space as _;
        unsafe {
            let cmsg = libc::CMSG_FIRSTHDR(&msg);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as _;
            std::ptr::write_unaligned(libc::CMSG_DATA(cmsg) as *mut libc::c_int, fd);
        }
        let sent = loop {
            let n = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, 0) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            break n as usize;
        };
        // 残りは普通に書く (記述子は最初の 1 バイトと一緒に届いている)
        (&*stream).write_all(&data[sent..])?;
        (&*stream).flush()
    }

    /// 使い回す前に、相手が閉じていないかを確かめる (サーバーが再起動したときなど)。
    /// 要求と要求の間に届くデータは無いはずなので、読めたら壊れているとみなす
    pub fn is_closed(stream: &Stream) -> bool {
        use std::io::Read;
        if stream.set_nonblocking(true).is_err() {
            return true;
        }
        let mut buf = [0u8; 1];
        let r = (&*stream).read(&mut buf);
        let closed = !matches!(&r, Err(e) if e.kind() == io::ErrorKind::WouldBlock);
        closed || stream.set_nonblocking(false).is_err()
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::time::Duration;

    use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use interprocess::TryClone;
    use juiz_sdk::anyhow::anyhow;

    use super::super::ipc_name::ipc_pipe_name;
    use crate::prelude::*;

    pub type Stream = interprocess::local_socket::Stream;

    pub struct Listener {
        inner: interprocess::local_socket::Listener,
        name: String,
    }

    /// 作ったユーザー (OWNER RIGHTS) と SYSTEM だけに許す。既定の記述子は Everyone に読み取りを許すため
    const PIPE_SDDL: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

    fn connect_pipe(pipe: &str) -> io::Result<Stream> {
        Stream::connect(pipe.to_fs_name::<GenericFilePath>()?)
    }

    pub fn connect(name: &str) -> JuizResult<Stream> {
        let pipe = ipc_pipe_name(name)?;
        connect_pipe(&pipe).map_err(|e| anyhow!("can not connect to ipc://{name} ({pipe}): {e}"))
    }

    pub fn bind(name: &str) -> JuizResult<Listener> {
        let pipe = ipc_pipe_name(name)?;
        if connect_pipe(&pipe).is_ok() {
            return Err(anyhow!(JuizError::InvalidValueError {
                message: format!("ipc://{name} is already in use ({pipe})")
            }));
        }
        let sddl: Vec<u16> = PIPE_SDDL.encode_utf16().chain(std::iter::once(0)).collect();
        let sddl = widestring::U16CStr::from_slice_truncate(&sddl)
            .map_err(|e| anyhow!("invalid security descriptor: {e}"))?;
        let sd = SecurityDescriptor::deserialize(sddl)?;
        let inner = ListenerOptions::new()
            .name(pipe.as_str().to_fs_name::<GenericFilePath>()?)
            .security_descriptor(sd)
            .create_sync()
            .map_err(|e| anyhow!("can not listen on ipc://{name} ({pipe}): {e}"))?;
        Ok(Listener { inner, name: pipe })
    }

    impl Listener {
        pub fn accept(&self) -> io::Result<Stream> {
            self.inner.accept()
        }

        pub fn wake(&self) {
            let _ = connect_pipe(&self.name);
        }

        pub fn location(&self) -> String {
            self.name.clone()
        }
    }

    /// 名前付きパイプには読み取りのタイムアウトが無いので、何もしない
    pub fn set_timeout(_stream: &Stream, _timeout: Option<Duration>) -> io::Result<()> {
        Ok(())
    }

    pub fn try_clone(stream: &Stream) -> io::Result<Stream> {
        stream.try_clone()
    }

    fn raw_handle(stream: &Stream) -> windows_sys::Win32::Foundation::HANDLE {
        use std::os::windows::io::{AsHandle, AsRawHandle};
        match stream {
            Stream::NamedPipe(s) => s.as_handle().as_raw_handle() as _,
        }
    }

    /// 受け手の読み取り。Windows では記述子を渡さない (共有メモリは名前で開く)
    pub struct RecvStream {
        inner: Stream,
    }

    impl RecvStream {
        pub fn new(inner: Stream) -> Self {
            RecvStream { inner }
        }
    }

    impl io::Read for RecvStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            io::Read::read(&mut self.inner, buf)
        }
    }

    /// サーバー側の接続を切る。相手 (クライアント) の次の読み書きは失敗し、
    /// この接続のスレッドで止まっている読み取りも取り消して終わらせる
    pub fn shutdown(stream: &Stream) {
        use windows_sys::Win32::System::{Pipes::DisconnectNamedPipe, IO::CancelIoEx};
        let handle = raw_handle(stream);
        unsafe {
            DisconnectNamedPipe(handle);
            CancelIoEx(handle, std::ptr::null());
        }
    }

    /// 相手が閉じていれば (サーバーが止まった、接続を切った) true。
    /// 読める量を覗くだけで、パイプの中身は読まない
    pub fn is_closed(stream: &Stream) -> bool {
        use windows_sys::Win32::System::Pipes::PeekNamedPipe;
        let mut available = 0u32;
        let ok = unsafe {
            PeekNamedPipe(
                raw_handle(stream),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        ok == 0
    }
}

pub use imp::*;
