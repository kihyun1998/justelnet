//! Cancelling `next_event` inside `select!` at random points loses nothing.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use justelnet::core::{self, OptionPolicy};
use justelnet::{Client, Event};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter, duplex};

/// The `server:` lines of the Core's Transcript `telnetd-netkit-rhel9.txt`.
const NETKIT_SERVER: &str = "\
server: ff fd 18 ff fd 20 ff fd 23 ff fd 27
server: ff fd 1f ff fb 01 ff fd 03 ff fb 03
server: ff fa 27 01 ff f0 ff fa 18 01 ff f0
server: ff fd 01 ff fb 05 ff fd 21
server: 6c 6f 63 61 6c 68 6f 73 74 20 6c 6f 67 69 6e 3a 20
";

/// The server's side of a netkit telnetd login on RHEL 9, then data with an
/// escaped IAC.
fn server_chunks() -> Vec<Vec<u8>> {
    let mut chunks: Vec<Vec<u8>> = NETKIT_SERVER
        .lines()
        .filter_map(|line| line.strip_prefix("server: "))
        .map(|hex| {
            hex.split_whitespace()
                .map(|b| u8::from_str_radix(b, 16).unwrap())
                .collect()
        })
        .collect();
    assert_eq!(chunks.len(), 5);
    chunks.push(b"Password: \xff\xff\r\n".to_vec());
    chunks
}

/// xorshift64*, so every run is reproducible from its seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) % n
    }
}

/// Ready on its `n`th poll after the first, waking itself in between.
struct AfterPolls(u64);

impl Future for AfterPolls {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 == 0 {
            return Poll::Ready(());
        }
        self.0 -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

async fn yield_times(n: u64) {
    for _ in 0..n {
        tokio::task::yield_now().await;
    }
}

/// What one run produced.
#[derive(Debug, PartialEq)]
struct Outcome {
    data: Vec<u8>,
    other_events: Vec<Event>,
    device_received: Vec<u8>,
}

/// Runs the device against a client over a 3-byte pipe, behind a 2-byte
/// write buffer so that flushes can be cut short too. The device writes and
/// reads in random pieces at random moments; with `cancel`, the client's
/// `next_event` races a future that wins after a random number of polls.
/// Returns the outcome and how many calls were cancelled.
async fn run(seed: u64, cancel: bool) -> (Outcome, u64) {
    let (stream, device) = duplex(3);
    let (mut device_read, mut device_write) = tokio::io::split(device);

    let mut rng = Rng::new(seed);
    let mut writer_rng = Rng::new(seed ^ 0x5eed);
    let writer = tokio::spawn(async move {
        for chunk in server_chunks() {
            let mut rest = &chunk[..];
            while !rest.is_empty() {
                let n = (1 + writer_rng.below(5) as usize).min(rest.len());
                device_write.write_all(&rest[..n]).await.unwrap();
                rest = &rest[n..];
                yield_times(writer_rng.below(4)).await;
            }
        }
        device_write.shutdown().await.unwrap();
    });
    let mut reader_rng = Rng::new(seed ^ 0xda7a);
    let reader = tokio::spawn(async move {
        let mut received = Vec::new();
        let mut buf = [0; 4];
        loop {
            let want = 1 + reader_rng.below(4) as usize;
            let n = device_read.read(&mut buf[..want]).await.unwrap();
            if n == 0 {
                return received;
            }
            received.extend_from_slice(&buf[..n]);
            yield_times(reader_rng.below(4)).await;
        }
    });

    let mut client = Client::new(BufWriter::with_capacity(2, stream), OptionPolicy::default());
    let mut data = Vec::new();
    let mut other_events = Vec::new();
    let mut cancelled = 0;
    loop {
        let event = if cancel && rng.below(4) != 0 {
            tokio::select! {
                biased;
                event = client.next_event() => event.unwrap(),
                () = AfterPolls(rng.below(12)) => {
                    cancelled += 1;
                    continue;
                }
            }
        } else {
            client.next_event().await.unwrap()
        };
        match event {
            Event::Core(core::Event::Data(bytes)) => data.extend(bytes),
            Event::Closed => break,
            event => other_events.push(event),
        }
    }
    drop(client);
    writer.await.unwrap();
    let device_received = reader.await.unwrap();
    let outcome = Outcome {
        data,
        other_events,
        device_received,
    };
    (outcome, cancelled)
}

#[tokio::test]
async fn cancelled_calls_lose_no_byte_either_way() {
    let (expected, _) = run(0, false).await;
    assert!(expected.data.ends_with(b"Password: \xff\r\n"));
    assert!(!expected.device_received.is_empty());

    let mut cancelled = 0;
    for seed in 1..=300 {
        let (outcome, n) = run(seed, true).await;
        assert_eq!(outcome, expected, "seed {seed}");
        cancelled += n;
    }
    assert!(cancelled > 1000, "only {cancelled} calls were cancelled");
}
