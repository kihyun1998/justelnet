//! Helpers shared by the fake-device tests.

use std::time::Duration;

use tokio::io::{AsyncReadExt, DuplexStream};

/// Every byte the device can read without waiting.
pub async fn received(device: &mut DuplexStream) -> Vec<u8> {
    let mut received = Vec::new();
    let mut buf = [0; 1024];
    while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(10), device.read(&mut buf)).await
    {
        if n == 0 {
            break;
        }
        received.extend_from_slice(&buf[..n]);
    }
    received
}
