use wsr_core::{
    content::{ContentPack, REMARK_COUNT, TextId},
    logic::{extensions::remarks::Remarks, newgame::LocalTime},
    pb::pbstr::PbStr,
};

fn fixture() -> ContentPack {
    ContentPack::from_entries(TextId::identifiers().enumerate().map(|(i, id)| {
        let value = if let Some(n) = id.binary_length() {
            PbStr(vec![0; n])
        } else if id == TextId::STARTUP_TICKER {
            PbStr(vec![0x80, 0xe9, 0, 13, 10])
        } else {
            PbStr::from(format!("Synthetic resource {i:06}                         ").as_str())
        };
        (id, value)
    }))
    .unwrap()
}

fn refresh_checksum(bytes: &mut [u8]) {
    let end = bytes.len() - 4;
    let mut crc = !0u32;
    for &byte in &bytes[..end] {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes[end..].copy_from_slice(&(!crc).to_le_bytes());
}

#[test]
fn catalog_preserves_bytes_and_rejects_incomplete_or_ambiguous_input() {
    let content = fixture();
    let bytes = content.to_bytes();
    assert_eq!(&bytes[..8], b"WSRCP002");
    let loaded = ContentPack::parse(&bytes).unwrap();
    assert_eq!(
        loaded.text(TextId::STARTUP_TICKER).as_bytes(),
        &[0x80, 0xe9, 0, 13, 10]
    );
    assert_eq!(loaded.to_bytes(), bytes);
    assert!(TextId::bankruptcy_remark(usize::MAX).is_none());
    for length in [0, 7, 11, bytes.len() - 1] {
        assert!(ContentPack::parse(&bytes[..length]).is_err());
    }
    let mut unknown = bytes.clone();
    unknown[12..16].copy_from_slice(&999u32.to_le_bytes());
    refresh_checksum(&mut unknown);
    assert!(
        ContentPack::parse(&unknown)
            .unwrap_err()
            .to_string()
            .contains("Unknown content identifier")
    );
    let mut duplicate = bytes.clone();
    duplicate[25..29].copy_from_slice(&1u32.to_le_bytes());
    refresh_checksum(&mut duplicate);
    assert!(
        ContentPack::parse(&duplicate)
            .unwrap_err()
            .to_string()
            .contains("Duplicate content identifier")
    );
    let mut oversized = bytes.clone();
    oversized[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    refresh_checksum(&mut oversized);
    assert!(
        ContentPack::parse(&oversized)
            .unwrap_err()
            .to_string()
            .contains("invalid length")
    );
    let mut corrupted = bytes.clone();
    corrupted[20] ^= 1;
    assert!(
        ContentPack::parse(&corrupted)
            .unwrap_err()
            .to_string()
            .contains("checksum")
    );
    let mut trailing = bytes;
    trailing.push(0);
    assert!(ContentPack::parse(&trailing).is_err());
    assert!(
        ContentPack::from_entries([(TextId::STARTUP_TICKER, PbStr::from("Synthetic"))]).is_err()
    );
}

#[test]
fn remark_selection_uses_the_supplied_catalog_and_refills_after_every_entry() {
    let content = fixture();
    let mut remarks = Remarks::default();
    for _ in 0..2 {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..REMARK_COUNT {
            let text = remarks.next(&LocalTime::HARNESS, &content);
            assert!(
                seen.insert(text),
                "Repeated a remark before exhausting the pool"
            );
        }
        assert_eq!(seen.len(), REMARK_COUNT);
    }
}
