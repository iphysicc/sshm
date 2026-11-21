//! Reachability check: opens a TCP connection and reads the SSH banner.
//! No authentication. Runs only at startup and on `r`: frequent unauthenticated
//! connections can trip fail2ban or OpenSSH PerSourcePenalties.

use std::io::Read;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum Status {
    Checking,
    Up {
        ms: u128,
        banner: Option<String>,
    },
    Down(Reason),
    /// Reached through a jump host, so it can't be probed directly.
    Skipped,
}

/// Translated when drawn, so switching language updates it too.
#[derive(Clone, Copy, Debug)]
pub enum Reason {
    Dns,
    Timeout,
    Refused,
    Unreachable,
}

impl Reason {
    pub fn text(self) -> &'static str {
        let t = crate::i18n::s();
        match self {
            Reason::Dns => t.dns_failed,
            Reason::Timeout => t.timed_out,
            Reason::Refused => t.refused,
            Reason::Unreachable => t.unreachable,
        }
    }
}

pub fn spawn(key: String, host: String, port: u16, tx: Sender<(String, Status)>) {
    thread::spawn(move || {
        let s = probe(&host, port);
        let _ = tx.send((key, s));
    });
}

fn probe(host: &str, port: u16) -> Status {
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(_) => return Status::Down(Reason::Dns),
    };
    let mut last = Reason::Unreachable;
    for addr in addrs {
        let t0 = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
            Ok(mut s) => {
                let ms = t0.elapsed().as_millis();
                let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0u8; 255];
                let banner = s.read(&mut buf).ok().and_then(|n| {
                    let text = String::from_utf8_lossy(&buf[..n]);
                    let line = text.lines().next()?.trim().to_string();
                    line.strip_prefix("SSH-2.0-")
                        .or_else(|| line.strip_prefix("SSH-1.99-"))
                        .map(|v| v.replace('_', " "))
                });
                return Status::Up { ms, banner };
            }
            Err(e) => {
                last = match e.kind() {
                    std::io::ErrorKind::TimedOut => Reason::Timeout,
                    std::io::ErrorKind::ConnectionRefused => Reason::Refused,
                    _ => Reason::Unreachable,
                }
            }
        }
    }
    Status::Down(last)
}
