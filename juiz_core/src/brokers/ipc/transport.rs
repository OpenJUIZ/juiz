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

    pub fn shutdown(_stream: &Stream) {}

    pub fn is_closed(_stream: &Stream) -> bool {
        false
    }
}

pub use imp::*;
