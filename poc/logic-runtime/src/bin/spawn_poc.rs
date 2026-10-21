//! Candidate B measurement: spawn a helper process and do 100 stdio round-trips.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Instant;

fn main() {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("echo-host")))
        .expect("echo-host path");

    let t0 = Instant::now();
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let spawn = t0.elapsed();

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    // First round-trip is the handshake.
    let t1 = Instant::now();
    writeln!(stdin, "{{\"id\":0}}").expect("write");
    stdin.flush().expect("flush");
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read");
    let handshake = t1.elapsed();

    let t2 = Instant::now();
    for i in 1..=100 {
        writeln!(stdin, "{{\"id\":{i}}}").expect("write");
        stdin.flush().expect("flush");
        line.clear();
        stdout.read_line(&mut line).expect("read");
    }
    let calls = t2.elapsed();

    let _ = child.kill();
    let _ = child.wait();
    println!(
        r#"{{"engine":"spawn","spawn_us":{},"handshake_us":{},"calls_100_us":{},"call_avg_us":{:.3}}}"#,
        spawn.as_micros(),
        handshake.as_micros(),
        calls.as_micros(),
        calls.as_micros() as f64 / 100.0
    );
}
