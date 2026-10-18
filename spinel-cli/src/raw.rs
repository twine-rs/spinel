use bytes::Bytes;

use spinel::{PosixSpinelHostHandle, PromiscuousMode, RawTxFrame};

/// Parse a hex string into bytes.
///
/// Operates on characters, so non-ASCII input reports an error instead of
/// splitting a multi-byte character.
fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();

    if !cleaned.len().is_multiple_of(2) {
        return Err("hex input needs an even number of digits".into());
    }

    cleaned
        .chunks(2)
        .enumerate()
        .map(
            |(i, pair)| match (pair[0].to_digit(16), pair[1].to_digit(16)) {
                (Some(hi), Some(lo)) => Ok((hi * 16 + lo) as u8),
                _ => Err(format!("invalid hex at offset {}", i * 2)),
            },
        )
        .collect()
}

/// Parse a promiscuous mode name.
fn parse_promiscuous(input: &str) -> Result<Option<PromiscuousMode>, String> {
    match input {
        "off" => Ok(Some(PromiscuousMode::Off)),
        "network" => Ok(Some(PromiscuousMode::Network)),
        "full" => Ok(Some(PromiscuousMode::Full)),
        other => Err(format!("unknown promiscuous mode: {other}")),
    }
}

/// Enable raw link mode, optionally transmit one frame, and print raw frames.
pub async fn run_raw(
    actor: PosixSpinelHostHandle,
    channel: u8,
    promiscuous: String,
    transmit: Option<String>,
) -> Result<(), String> {
    let promiscuous = parse_promiscuous(&promiscuous)?;

    // Subscribe before enabling raw mode. The device starts forwarding
    // frames as soon as the host enables the mode, and a broadcast with
    // no receiver drops them.
    let mut raw_rx = actor
        .subscribe_raw_rx()
        .await
        .map_err(|e| format!("raw subscribe failed: {e}"))?;

    actor
        .enable_raw(channel, promiscuous)
        .await
        .map_err(|e| format!("enable raw failed: {e}"))?;
    println!("raw mode enabled on channel {channel}");

    if let Some(hex) = transmit {
        let psdu = parse_hex(&hex)?;
        let frame = RawTxFrame {
            psdu: Bytes::from(psdu),
            channel,
            config: Default::default(),
        };

        let done = actor
            .transmit_raw(frame)
            .await
            .map_err(|e| format!("transmit failed: {e}"))?;
        println!(
            "transmit done, status {}, frame pending {}, ack {:?}",
            done.status, done.frame_pending, done.ack
        );
    }

    loop {
        match raw_rx.recv().await {
            Ok(frame) => {
                let psdu: Vec<String> = frame.psdu.iter().map(|b| format!("{b:02x}")).collect();
                println!(
                    "rx {} bytes, rssi {}, lqi {:?}, channel {:?}, psdu {}",
                    frame.psdu.len(),
                    frame.rssi,
                    frame.lqi,
                    frame.channel,
                    psdu.join("")
                );
            }
            Err(e) => {
                log::warn!("raw receive lagged: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hex;

    #[test]
    fn parse_hex_decodes_bytes() {
        assert_eq!(parse_hex("0aFF 01"), Ok(vec![0x0A, 0xFF, 0x01]));
    }

    #[test]
    fn parse_hex_rejects_odd_length() {
        assert!(parse_hex("abc").is_err());
    }

    #[test]
    fn parse_hex_rejects_invalid_digits() {
        assert!(parse_hex("zz").is_err());
    }

    #[test]
    fn parse_hex_rejects_multi_byte_characters() {
        // A multi-byte character reports an error instead of panicking on a
        // character boundary.
        assert!(parse_hex("\u{e9}a").is_err());
    }
}
