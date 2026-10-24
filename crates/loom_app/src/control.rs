//! Line-delimited JSON control socket.
//!
//! A background thread accepts localhost connections and forwards each request
//! to the UI thread, which owns `LoomApp`. The connection thread blocks on the
//! reply channel so every request gets exactly one response line.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

pub struct Request {
    pub body: serde_json::Value,
    pub reply: Sender<serde_json::Value>,
}

/// Binds `127.0.0.1:port` and returns the receiver the UI thread drains.
pub fn spawn(port: u16) -> std::io::Result<Receiver<Request>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (tx, rx) = channel::<Request>();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let tx = tx.clone();
            thread::spawn(move || {
                let mut out = match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let body: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = writeln!(out, "{}", err(&format!("bad json: {e}")));
                            continue;
                        }
                    };
                    let (rtx, rrx) = channel();
                    if tx
                        .send(Request {
                            body,
                            reply: rtx,
                        })
                        .is_err()
                    {
                        break;
                    }
                    let resp = match rrx.recv() {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    if writeln!(out, "{resp}").is_err() {
                        break;
                    }
                }
            });
        }
    });
    Ok(rx)
}

fn err(msg: &str) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": msg })
}
