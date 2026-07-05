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
    pub pttype: Option<String>,
    pub ptuuid: Option<String>,
    #[serde(default)]
    pub mountpoints: Vec<Option<String>>,
    pub fstype: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub children: Vec<BlockDevice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SfdiskDocument {
    pub partitiontable: SfdiskTable,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SfdiskTable {
    pub label: String,
    pub id: String,
    pub partitions: Vec<SfdiskPartition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SfdiskPartition {
    pub start: u64,
    pub size: u64,
    #[serde(rename = "type")]
    pub partition_type: String,
    #[serde(default)]
    pub bootable: bool,
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
    #[error("partition table is not the expected MBR layout")]
    PartitionTableMismatch,
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
    sfdisk: &SfdiskDocument,
    boot_major_minor: &str,
) -> Result<VerifiedLayout, LayoutError> {
    if manifest.schema != "rigos.image-layout/v2" || manifest.partition_table != "mbr" {
        return Err(LayoutError::PartitionTableMismatch);
    }
    if sfdisk.partitiontable.label != "dos"
        || !eq_disk_id(&sfdisk.partitiontable.id, &manifest.disk_guid)
    {
        return Err(LayoutError::PartitionTableMismatch);
    }

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
    if disk.pttype.as_deref() != Some("dos")
        || !disk
            .ptuuid
            .as_deref()
            .is_some_and(|value| eq_disk_id(value, &manifest.disk_guid))
    {
        return Err(LayoutError::PartitionTableMismatch);
    }
    if disk.size < manifest.minimum_media_size_bytes {
        return Err(LayoutError::UndersizedMedia);
    }
    if disk.children.len() != manifest.partitions.len()
        || sfdisk.partitiontable.partitions.len() != manifest.partitions.len()
    {
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
        let label_matches = if number == manifest.final_state_partition {
            matches!(
                child.label.as_deref(),
                Some("RIGOS_STATE_SEED" | "RIGOS_STATE")
            )
        } else {
            child.label.as_deref() == Some(contract.label.as_str())
        };
        if child.device_type != "part"
            || !label_matches
            || !eq_mbr_type(child.parttype.as_deref(), &contract.type_guid)
            || !eq_id(child.partuuid.as_deref(), &contract.unique_guid)
            || child.start != Some(contract.start_lba)
            || child.size / u64::from(manifest.logical_sector_size) < contract.minimum_size_lba
        {
            return Err(LayoutError::PartitionMismatch(number));
        }
    }

    for (offset, observed_partition) in sfdisk.partitiontable.partitions.iter().enumerate() {
        let number = u32::try_from(offset + 1).map_err(|_| LayoutError::PartitionSetMismatch)?;
        let contract = expected
            .get(&number)
            .ok_or(LayoutError::PartitionSetMismatch)?;
        if observed_partition.start != contract.start_lba
            || observed_partition.size < contract.minimum_size_lba
            || !eq_mbr_type(
                Some(&observed_partition.partition_type),
                &contract.type_guid,
            )
            || observed_partition.bootable != (number == 1)
        {
            return Err(LayoutError::PartitionMismatch(number));
        }
    }

    let root_is_expected = disk.children.iter().any(|child| {
        child.major_minor == boot_major_minor
            && matches!(
                child.label.as_deref(),
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
    let state_label_matches = matches!(
        state.label.as_deref(),
        Some("RIGOS_STATE_SEED" | "RIGOS_STATE")
    );
    if state.start != max_start || !state_label_matches {
        return Err(LayoutError::StateNotFinal);
    }
    if disk.children.iter().any(|child| {
        child.major_minor != boot_major_minor
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

fn eq_id(observed: Option<&str>, expected: &str) -> bool {
    observed.is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn strip_hex_prefix(value: &str) -> &str {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
}

fn eq_disk_id(observed: &str, expected: &str) -> bool {
    strip_hex_prefix(observed).eq_ignore_ascii_case(strip_hex_prefix(expected))
}

fn eq_mbr_type(observed: Option<&str>, expected: &str) -> bool {
    fn parse(value: &str) -> Option<u8> {
        u8::from_str_radix(strip_hex_prefix(value), 16).ok()
    }
    observed.and_then(parse) == parse(expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigos_schema::ImagePartitionV1;

    fn manifest() -> ImageLayoutV1 {
        ImageLayoutV1 {
            schema: "rigos.image-layout/v2".into(),
            image_version: "0.0.4-alpha.2".into(),
            image_id: "rigos-usb-amd64".into(),
            partition_table: "mbr".into(),
            disk_guid: "0x5249474f".into(),
            logical_sector_size: 512,
            minimum_media_size_bytes: 8_000_000_000,
            alignment_lba: 2048,
            final_state_partition: 4,
            build_commit: "commit".into(),
            root_payload_sha256: "hash".into(),
            partitions: vec![
                partition(1, "EFI_SYSTEM", "0x0c", "5249474f-01", 2048, 524288),
                partition(2, "RIGOS_ROOT_A", "0x83", "5249474f-02", 526336, 2097152),
                partition(3, "RIGOS_ROOT_B", "0x83", "5249474f-03", 2623488, 2097152),
                partition(
                    4,
                    "RIGOS_STATE_SEED",
                    "0x83",
                    "5249474f-04",
                    4720640,
                    524288,
                ),
            ],
        }
    }

    fn partition(
        number: u32,
        label: &str,
        partition_type: &str,
        partuuid: &str,
        start_lba: u64,
        size_lba: u64,
    ) -> ImagePartitionV1 {
        ImagePartitionV1 {
            number,
            label: label.into(),
            type_guid: partition_type.into(),
            unique_guid: partuuid.into(),
            start_lba,
            minimum_size_lba: size_lba,
            filesystem: Some(if number == 1 { "fat32" } else { "ext4" }.into()),
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
                pttype: Some("dos".into()),
                ptuuid: Some("5249474f".into()),
                mountpoints: vec![],
                fstype: None,
                label: None,
                children: contract
                    .partitions
                    .iter()
                    .map(|partition| BlockDevice {
                        major_minor: format!("8:{}", partition.number),
                        path: format!("/dev/sda{}", partition.number),
                        device_type: "part".into(),
                        size: partition.minimum_size_lba * 512,
                        ro: false,
                        tran: None,
                        partn: Some(partition.number),
                        parttype: Some(partition.type_guid.clone()),
                        partuuid: Some(partition.unique_guid.clone()),
                        partlabel: None,
                        start: Some(partition.start_lba),
                        pttype: None,
                        ptuuid: None,
                        mountpoints: if partition.number == 2 {
                            vec![Some("/run/live/medium".into())]
                        } else {
                            vec![]
                        },
                        fstype: partition.filesystem.clone(),
                        label: Some(partition.label.clone()),
                        children: vec![],
                    })
                    .collect(),
            }],
        }
    }

    fn sfdisk() -> SfdiskDocument {
        let contract = manifest();
        SfdiskDocument {
            partitiontable: SfdiskTable {
                label: "dos".into(),
                id: "0x5249474f".into(),
                partitions: contract
                    .partitions
                    .iter()
                    .map(|partition| SfdiskPartition {
                        start: partition.start_lba,
                        size: partition.minimum_size_lba,
                        partition_type: partition.type_guid.clone(),
                        bootable: partition.number == 1,
                    })
                    .collect(),
            },
        }
    }

    #[test]
    fn exact_layout_passes() {
        assert!(validate_layout(&manifest(), &observed(), &sfdisk(), "8:2").is_ok());
    }

    #[test]
    fn initialized_state_label_passes() {
        let mut devices = observed();
        devices.blockdevices[0].children[3].label = Some("RIGOS_STATE".into());
        assert!(validate_layout(&manifest(), &devices, &sfdisk(), "8:2").is_ok());
    }

    #[test]
    fn moved_state_start_fails() {
        let mut devices = observed();
        devices.blockdevices[0].children[3].start = Some(999);
        assert_eq!(
            validate_layout(&manifest(), &devices, &sfdisk(), "8:2"),
            Err(LayoutError::PartitionMismatch(4))
        );
    }

    #[test]
    fn non_usb_fails() {
        let mut devices = observed();
        devices.blockdevices[0].tran = Some("sata".into());
        assert_eq!(
            validate_layout(&manifest(), &devices, &sfdisk(), "8:2"),
            Err(LayoutError::NotWritableUsb)
        );
    }

    #[test]
    fn extra_partition_fails() {
        let mut devices = observed();
        let extra = devices.blockdevices[0].children[0].clone();
        devices.blockdevices[0].children.push(extra);
        assert_eq!(
            validate_layout(&manifest(), &devices, &sfdisk(), "8:2"),
            Err(LayoutError::PartitionSetMismatch)
        );
    }

    #[test]
    fn wrong_disk_signature_fails() {
        let mut table = sfdisk();
        table.partitiontable.id = "0xdeadbeef".into();
        assert_eq!(
            validate_layout(&manifest(), &observed(), &table, "8:2"),
            Err(LayoutError::PartitionTableMismatch)
        );
    }

    #[test]
    fn inactive_efi_partition_fails() {
        let mut table = sfdisk();
        table.partitiontable.partitions[0].bootable = false;
        assert_eq!(
            validate_layout(&manifest(), &observed(), &table, "8:2"),
            Err(LayoutError::PartitionMismatch(1))
        );
    }

    #[test]
    fn read_only_media_fails() {
        let mut devices = observed();
        devices.blockdevices[0].ro = true;
        assert_eq!(
            validate_layout(&manifest(), &devices, &sfdisk(), "8:2"),
            Err(LayoutError::NotWritableUsb)
        );
    }

    #[test]
    fn undersized_media_fails() {
        let mut devices = observed();
        devices.blockdevices[0].size = 7_999_999_999;
        assert_eq!(
            validate_layout(&manifest(), &devices, &sfdisk(), "8:2"),
            Err(LayoutError::UndersizedMedia)
        );
    }
}
