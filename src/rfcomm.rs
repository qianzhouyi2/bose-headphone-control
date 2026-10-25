use crate::{
    ble::check_cancelled,
    protocol::{Decoder, Frame},
};
use anyhow::{bail, ensure, Result};
use std::{
    collections::VecDeque,
    mem::MaybeUninit,
    time::{Duration, Instant},
};

const AF_BTH: i32 = 32;
const SOCK_STREAM: i32 = 1;
const BTPROTO_RFCOMM: i32 = 3;
const SOCKET_ERROR: i32 = -1;
const WSAEINTR: i32 = 10004;
const WSAEWOULDBLOCK: i32 = 10035;
const WSAEINPROGRESS: i32 = 10036;
const FIONBIO: u32 = 0x8004_667e;

#[repr(C)]
struct FdSet {
    count: u32,
    sockets: [usize; 64],
}

impl FdSet {
    fn one(socket: usize) -> Self {
        let mut set = Self {
            count: 1,
            sockets: [0; 64],
        };
        set.sockets[0] = socket;
        set
    }
}

#[repr(C)]
struct TimeVal {
    seconds: i32,
    microseconds: i32,
}

#[link(name = "Ws2_32")]
extern "system" {
    fn WSAStartup(version: u16, data: *mut u8) -> i32;
    fn WSACleanup() -> i32;
    fn WSAGetLastError() -> i32;
    fn socket(domain: i32, kind: i32, protocol: i32) -> usize;
    fn closesocket(socket: usize) -> i32;
    fn ioctlsocket(socket: usize, command: u32, value: *mut u32) -> i32;
    fn connect(socket: usize, address: *const u8, length: i32) -> i32;
    fn select(
        ignored: i32,
        read: *mut FdSet,
        write: *mut FdSet,
        except: *mut FdSet,
        timeout: *const TimeVal,
    ) -> i32;
    fn getsockopt(socket: usize, level: i32, option: i32, value: *mut i8, length: *mut i32) -> i32;
    fn send(socket: usize, buffer: *const i8, length: i32, flags: i32) -> i32;
    fn recv(socket: usize, buffer: *mut i8, length: i32, flags: i32) -> i32;
}

struct Winsock;

impl Winsock {
    fn start() -> Result<Self> {
        let mut data = MaybeUninit::<[usize; 64]>::zeroed();
        let result = unsafe { WSAStartup(0x0202, data.as_mut_ptr().cast()) };
        ensure!(result == 0, "Winsock initialization failed ({result})");
        Ok(Self)
    }
}

impl Drop for Winsock {
    fn drop(&mut self) {
        unsafe {
            let _ = WSACleanup();
        }
    }
}

struct Socket(usize);

impl Socket {
    fn open() -> Result<Self> {
        let socket = unsafe { socket(AF_BTH, SOCK_STREAM, BTPROTO_RFCOMM) };
        ensure!(
            socket != usize::MAX,
            "RFCOMM socket creation failed ({})",
            last_error()
        );
        Ok(Self(socket))
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            let _ = closesocket(self.0);
        }
    }
}

pub struct RfcommTransport {
    socket: Socket,
    _winsock: Winsock,
    decoder: Decoder,
    frames: VecDeque<Frame>,
}

impl RfcommTransport {
    pub fn connect(address: u64, channel: u32) -> Result<Self> {
        ensure!((1..=30).contains(&channel), "Invalid RFCOMM channel");
        let winsock = Winsock::start()?;
        let socket = Socket::open()?;
        let mut nonblocking = 1u32;
        ensure!(
            unsafe { ioctlsocket(socket.0, FIONBIO, &mut nonblocking) } != SOCKET_ERROR,
            "Could not configure RFCOMM socket ({})",
            last_error()
        );

        let mut address_bytes = [0u8; 30];
        address_bytes[..2].copy_from_slice(&(AF_BTH as u16).to_le_bytes());
        address_bytes[2..10].copy_from_slice(&address.to_le_bytes());
        address_bytes[26..30].copy_from_slice(&channel.to_le_bytes());
        let result =
            unsafe { connect(socket.0, address_bytes.as_ptr(), address_bytes.len() as i32) };
        if result == SOCKET_ERROR {
            let error = last_error();
            ensure!(
                matches!(error, WSAEWOULDBLOCK | WSAEINPROGRESS | WSAEINTR),
                "RFCOMM connection failed ({error})"
            );
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            check_cancelled()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(!remaining.is_zero(), "RFCOMM connection timed out");
            if wait_socket(
                socket.0,
                false,
                true,
                remaining.min(Duration::from_millis(50)),
            )? {
                break;
            }
        }
        let mut error = 0i32;
        let mut length = std::mem::size_of::<i32>() as i32;
        ensure!(
            unsafe {
                getsockopt(
                    socket.0,
                    0xffff,
                    0x1007,
                    (&mut error as *mut i32).cast(),
                    &mut length,
                )
            } != SOCKET_ERROR,
            "RFCOMM connection check failed ({})",
            last_error()
        );
        ensure!(error == 0, "RFCOMM connection failed ({error})");
        check_cancelled()?;

        Ok(Self {
            socket,
            _winsock: winsock,
            decoder: Decoder::default(),
            frames: VecDeque::new(),
        })
    }

    pub fn send(&mut self, packet: &[u8]) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut offset = 0;
        while offset < packet.len() {
            check_cancelled()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(!remaining.is_zero(), "RFCOMM write timed out");
            if !wait_socket(
                self.socket.0,
                false,
                true,
                remaining.min(Duration::from_millis(50)),
            )? {
                continue;
            }
            let count = unsafe {
                send(
                    self.socket.0,
                    packet[offset..].as_ptr().cast(),
                    (packet.len() - offset) as i32,
                    0,
                )
            };
            if count == SOCKET_ERROR {
                let error = last_error();
                if matches!(error, WSAEWOULDBLOCK | WSAEINTR) {
                    continue;
                }
                bail!("RFCOMM write failed ({error})");
            }
            ensure!(count > 0, "RFCOMM connection closed while writing");
            offset += count as usize;
        }
        Ok(())
    }

    pub fn recv_frame(&mut self, timeout: Duration) -> Result<Option<Frame>> {
        if let Some(frame) = self.frames.pop_front() {
            return Ok(Some(frame));
        }
        let deadline = Instant::now() + timeout;
        loop {
            check_cancelled()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            if !wait_socket(
                self.socket.0,
                true,
                false,
                remaining.min(Duration::from_millis(50)),
            )? {
                continue;
            }
            let mut buffer = [0i8; 4096];
            let count = unsafe { recv(self.socket.0, buffer.as_mut_ptr(), buffer.len() as i32, 0) };
            if count == SOCKET_ERROR {
                let error = last_error();
                if matches!(error, WSAEWOULDBLOCK | WSAEINTR) {
                    continue;
                }
                bail!("RFCOMM read failed ({error})");
            }
            ensure!(count > 0, "RFCOMM connection closed");
            let bytes =
                unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), count as usize) };
            self.frames.extend(self.decoder.push(bytes)?);
            if let Some(frame) = self.frames.pop_front() {
                return Ok(Some(frame));
            }
        }
    }

    pub fn discard_ready(&mut self) -> Result<()> {
        self.frames.clear();
        loop {
            check_cancelled()?;
            if !wait_socket(self.socket.0, true, false, Duration::ZERO)? {
                return Ok(());
            }
            let mut buffer = [0i8; 4096];
            let count = unsafe { recv(self.socket.0, buffer.as_mut_ptr(), buffer.len() as i32, 0) };
            if count == SOCKET_ERROR {
                let error = last_error();
                if matches!(error, WSAEWOULDBLOCK | WSAEINTR) {
                    return Ok(());
                }
                bail!("RFCOMM read failed ({error})");
            }
            ensure!(count > 0, "RFCOMM connection closed");
            let bytes =
                unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), count as usize) };
            let _ = self.decoder.push(bytes)?;
        }
    }
}

fn wait_socket(socket: usize, readable: bool, writable: bool, timeout: Duration) -> Result<bool> {
    let mut read_set = FdSet::one(socket);
    let mut write_set = FdSet::one(socket);
    let mut except_set = FdSet::one(socket);
    let time = TimeVal {
        seconds: timeout.as_secs().min(i32::MAX as u64) as i32,
        microseconds: timeout.subsec_micros() as i32,
    };
    let result = unsafe {
        select(
            0,
            if readable {
                &mut read_set
            } else {
                std::ptr::null_mut()
            },
            if writable {
                &mut write_set
            } else {
                std::ptr::null_mut()
            },
            if writable {
                &mut except_set
            } else {
                std::ptr::null_mut()
            },
            &time,
        )
    };
    if result == SOCKET_ERROR {
        let error = last_error();
        if error == WSAEINTR {
            return Ok(false);
        }
        bail!("RFCOMM socket wait failed ({error})");
    }
    Ok(result > 0)
}

fn last_error() -> i32 {
    unsafe { WSAGetLastError() }
}
