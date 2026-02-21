use std::io;

#[derive(Debug, Clone, Copy)]
pub struct PeerIdentity {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub pid: Option<i32>,
}

#[cfg(unix)]
pub use self::unix::get_peer_identity;

#[cfg(windows)]
pub use self::windows::get_peer_identity;

#[cfg(unix)]
mod unix {
    use std::os::unix::io::AsRawFd;
    use super::PeerIdentity;
    use std::io;

    #[cfg(target_os = "linux")]
    pub fn get_peer_identity<T: AsRawFd>(stream: &T) -> io::Result<PeerIdentity> {
        use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
        
        // This requires nix::sys::socket::sockopt::PeerCredentials which is Linux only.
        // And nix feature "socket".
        
        let fd = stream.as_raw_fd();
        // nix wrappers for PeerCredentials return ucred struct
        let cred = getsockopt(fd, PeerCredentials).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(PeerIdentity {
            uid: Some(cred.uid()),
            gid: Some(cred.gid()),
            pid: Some(cred.pid()),
        })
    }

    #[cfg(target_os = "macos")]
    pub fn get_peer_identity<T: AsRawFd>(stream: &T) -> io::Result<PeerIdentity> {
        let fd = stream.as_raw_fd();
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;

        let ret = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(PeerIdentity {
            uid: Some(uid as u32),
            gid: Some(gid as u32),
            pid: None,
        })
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::io::AsRawHandle;
    use super::PeerIdentity;
    use std::io;
    use windows::Win32::System::Pipes::GetNamedPipeClientProcessId;
    use windows::Win32::Foundation::HANDLE;

    pub fn get_peer_identity<T: AsRawHandle>(stream: &T) -> io::Result<PeerIdentity> {
        let handle = stream.as_raw_handle();
        let mut client_pid = 0;
        
        let res = unsafe {
            GetNamedPipeClientProcessId(HANDLE(handle as isize), &mut client_pid)
        };

        if res.as_bool() {
             Ok(PeerIdentity {
                uid: None,
                gid: None,
                pid: Some(client_pid as i32),
            })
        } else {
             Err(io::Error::last_os_error())
        }
    }
}
