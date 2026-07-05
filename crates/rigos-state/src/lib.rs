#![forbid(unsafe_code)]

use rigos_schema::ImageLayoutV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct LsblkDocument {
    pub blockdevices: Vec<BlockDevice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockDevice {
    #[serde(rename = "maj:min")]
    pub major_minor: String,
    pub path: String,
    #[serde(rename = "type")]
    pub device_type: String,
    pub size: u64,
    pub ro: bool,
    pub tran: Option<String>,
    pub partn: Option<u32>,
    pub parttype: Option<String>,
    pub partuuid: Option<String>,
    pub partlabel: Option<String>,
    pub start: Option<u64>,
    #[serde(default)]
    pub mountpoints: Vec<Option<String>>,
    pub fstype: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub children: Vec<BlockDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedLayout {
    pub disk_path: String,
    pub disk_size_bytes: u64,
    pub state_path: String,
    pub state_start_lba: u64,
    pub state_size_lba: u64,
    pub state_unique_guid: String,
    pub state_type_guid: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateOutcome {
    Ready,
    Grown,
    LimitedCapacity,
    Stateless,
    BlockedLayoutMismatch,
    BlockedAmbiguousBootDevice,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("exact boot parent could not be proven")]
    AmbiguousBootDevice,
    #[error("boot parent is not a writable USB disk")]
    NotWritableUsb,
    #[error("media is smaller than the image contract")]
    UndersizedMedia,
    #[error("observed partition set differs from the image manifest")]
    PartitionSetMismatch,
    #[error("partition {0} differs from the immutable image manifest")]
    PartitionMismatch(u32),
    #[error("RIGOS_STATE_SEED is not the final partition")]
    StateNotFinal,
    #[error("boot source is not ROOT_A or ROOT_B on the verified disk")]
    RootSourceMismatch,
    #[error("an unexpected writable child mount exists")]
    UnexpectedWritableMount,
}

pub fn validate_layout(
    manifest: &ImageLayoutV1,
    observed: &LsblkDocument,
    boot_major_minor: &str,
) -> Result<VerifiedLayout, LayoutError> {
    let disk = observed
        .blockdevices
        .iter()
        .find(|device| {
            device.device_type == "disk"
                && device
                    .children
                    .iter()
                    .any(|child| child.major_minor == boot_major_minor)
        })
        .ok_or(LayoutError::AmbiguousBootDevice)?;
    if disk.tran.as_deref() != Some("usb") || disk.ro {
        return Err(LayoutError::NotWritableUsb);
    }
    if disk.size < manifest.minimum_media_size_bytes {
        return Err(LayoutError::UndersizedMedia);
    }
    if disk.children.len() != manifest.partitions.len() {
        return Err(LayoutError::PartitionSetMismatch);
    }
    let expected: BTreeMap<u32, _> = manifest
        .partitions
        .iter()
        .map(|partition| (partition.number, partition))
        .collect();
    for child in &disk.children {
        let number = child.partn.ok_or(LayoutError::PartitionSetMismatch)?;
        let contract = expected
            .get(&number)
            .ok_or(LayoutError::PartitionSetMismatch)?;
        if child.device_type != "part"
            || child.partlabel.as_deref() != Some(contract.label.as_str())
            || !eq_guid(child.parttype.as_deref(), &contract.type_guid)
            || !eq_guid(child.partuuid.as_deref(), &contract.unique_guid)
            || child.start != Some(contract.start_lba)
            || child.size / u64::from(manifest.logical_sector_size) < contract.minimum_size_lba
        {
            return Err(LayoutError::PartitionMismatch(number));
        }
    }
    let root_is_expected = disk.children.iter().any(|child| {
        child.major_minor == boot_major_minor
            && matches!(
                child.partlabel.as_deref(),
                Some("RIGOS_ROOT_A" | "RIGOS_ROOT_B")
            )
    });
    if !root_is_expected {
        return Err(LayoutError::RootSourceMismatch);
    }
    let state = disk
        .children
        .iter()
        .find(|child| child.partn == Some(manifest.final_state_partition))
        .ok_or(LayoutError::StateNotFinal)?;
    let max_start = disk.children.iter().filter_map(|child| child.start).max();
    if state.start != max_start || state.partlabel.as_deref() != Some("RIGOS_STATE_SEED") {
        return Err(LayoutError::StateNotFinal);
    }
    if disk.children.iter().any(|child| {
        child.partn != Some(2)
            && child.major_minor != boot_major_minor
            && child
                .mountpoints
                .iter()
                .flatten()
                .any(|mount| !mount.is_empty())
    }) {
        return Err(LayoutError::UnexpectedWritableMount);
    }
    Ok(VerifiedLayout {
        disk_path: disk.path.clone(),
        disk_size_bytes: disk.size,
        state_path: state.path.clone(),
        state_start_lba: state.start.unwrap_or_default(),
        state_size_lba: state.size / u64::from(manifest.logical_sector_size),
        state_unique_guid: state.partuuid.clone().unwrap_or_default(),
        state_type_guid: state.parttype.clone().unwrap_or_default(),
    })
}

fn eq_guid(observed: Option<&str>, expected: &str) -> bool {
    observed.is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigos_schema::ImagePartitionV1;

    fn manifest() -> ImageLayoutV1 {
        ImageLayoutV1 {
            schema: "rigos.image-layout/v1".into(),
            image_version: "0.0.4-alpha.1".into(),
            image_id: "rigos-usb-amd64".into(),
            partition_table: "gpt".into(),
            disk_guid: "disk".into(),
            logical_sector_size: 512,
            minimum_media_size_bytes: 8_000_000_000,
            alignment_lba: 2048,
            final_state_partition: 5,
            build_commit: "commit".into(),
            root_payload_sha256: "hash".into(),
            partitions: (1..=5)
                .map(|number| ImagePartitionV1 {
                    number,
                    label: match number {
                        3 => "RIGOS_ROOT_A",
                        4 => "RIGOS_ROOT_B",
                        5 => "RIGOS_STATE_SEED",
                        _ => "OTHER",
                    }
                    .into(),
                    type_guid: format!("type-{number}"),
                    unique_guid: format!("uuid-{number}"),
                    start_lba: u64::from(number) * 2048,
                    minimum_size_lba: 1024,
                    filesystem: None,
                })
                .collect(),
        }
    }

    fn observed() -> LsblkDocument {
        let contract = manifest();
        LsblkDocument {
            blockdevices: vec![BlockDevice {
                major_minor: "8:0".into(),
                path: "/dev/sda".into(),
                device_type: "disk".into(),
                size: 16_000_000_000,
                ro: false,
                tran: Some("usb".into()),
                partn: None,
                parttype: None,
                partuuid: None,
                partlabel: None,
                start: None,
                mountpoints: vec![],
                fstype: None,
                label: None,
                children: contract
                    .partitions
                    .iter()
                    .map(|p| BlockDevice {
                        major_minor: format!("8:{}", p.number),
                        path: format!("/dev/sda{}", p.number),
                        device_type: "part".into(),
                        size: p.minimum_size_lba * 512,
                        ro: false,
                        tran: None,
                        partn: Some(p.number),
                        parttype: Some(p.type_guid.clone()),
                        partuuid: Some(p.unique_guid.clone()),
                        partlabel: Some(p.label.clone()),
                        start: Some(p.start_lba),
                        mountpoints: if p.number == 3 {
                            vec![Some("/run/live/medium".into())]
                        } else {
                            vec![]
                        },
                        fstype: None,
                        label: None,
                        children: vec![],
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn exact_layout_passes() {
        assert!(validate_layout(&manifest(), &observed(), "8:3").is_ok());
    }
    #[test]
    fn moved_state_start_fails() {
        let mut o = observed();
        o.blockdevices[0].children[4].start = Some(999);
        assert_eq!(
            validate_layout(&manifest(), &o, "8:3"),
            Err(LayoutError::PartitionMismatch(5))
        );
    }
    #[test]
    fn non_usb_fails() {
        let mut o = observed();
        o.blockdevices[0].tran = Some("sata".into());
        assert_eq!(
            validate_layout(&manifest(), &o, "8:3"),
            Err(LayoutError::NotWritableUsb)
        );
    }
    #[test]
    fn extra_partition_fails() {
        let mut o = observed();
        let extra = o.blockdevices[0].children[0].clone();
        o.blockdevices[0].children.push(extra);
        assert_eq!(
            validate_layout(&manifest(), &o, "8:3"),
            Err(LayoutError::PartitionSetMismatch)
        );
    }

    #[test]
    fn wrong_partition_guid_fails() {
        let mut o = observed();
        o.blockdevices[0].children[4].partuuid = Some("unexpected".into());
        assert_eq!(
            validate_layout(&manifest(), &o, "8:3"),
            Err(LayoutError::PartitionMismatch(5))
        );
    }

    #[test]
    fn read_only_media_fails() {
        let mut o = observed();
        o.blockdevices[0].ro = true;
        assert_eq!(
            validate_layout(&manifest(), &o, "8:3"),
            Err(LayoutError::NotWritableUsb)
        );
    }

    #[test]
    fn undersized_media_fails() {
        let mut o = observed();
        o.blockdevices[0].size = 7_999_999_999;
        assert_eq!(
            validate_layout(&manifest(), &o, "8:3"),
            Err(LayoutError::UndersizedMedia)
        );
    }
}
