#![forbid(unsafe_code)]

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProductContract {
    pub product: String,
    pub architecture: String,
    pub runtime_medium: String,
    pub recommended_capacity_gib: u32,
    pub minimum_usb_interface: String,
    pub legacy_bios: bool,
    pub uefi: bool,
    pub internal_disk_install: bool,
    pub internal_disk_auto_mount: bool,
    pub internal_disk_auto_format: bool,
    pub internal_disk_swap: bool,
    pub cloud_account_required: bool,
    pub subscription_required: bool,
    pub activation_required: bool,
    pub license_server_required: bool,
    pub worker_limit: u64,
    pub rigos_dev_fee_percent: u32,
    pub forced_pool: bool,
    pub paths: ProductPaths,
    pub usb: UsbContract,
}

#[derive(Debug, Deserialize)]
pub struct ProductPaths {
    pub config: String,
    pub state: String,
    pub runtime: String,
    pub libraries: String,
    pub service: String,
}

#[derive(Debug, Deserialize)]
pub struct UsbContract {
    pub temporary_filesystems: Vec<String>,
    pub root_slots: Vec<String>,
    pub state_partition: String,
    pub root_read_only: bool,
    pub zram_swap: bool,
    pub bounded_journal: bool,
    pub bounded_event_log: bool,
    pub atomic_policy_writes: bool,
    pub checksummed_policy_revisions: bool,
}

pub fn embedded_contract() -> ProductContract {
    toml::from_str(include_str!("../../../configs/product-contract.toml"))
        .expect("checked-in product contract must parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_identity_and_paths_are_locked() {
        let contract = embedded_contract();
        assert_eq!(contract.product, "RIGOS");
        assert_eq!(contract.paths.config, "/etc/rigos");
        assert_eq!(contract.paths.state, "/var/lib/rigos");
        assert_eq!(contract.paths.runtime, "/run/rigos");
        assert_eq!(contract.paths.libraries, "/usr/lib/rigos");
        assert_eq!(contract.paths.service, "rigosd.service");
    }

    #[test]
    fn no_billing_account_or_forced_pool_path_exists() {
        let contract = embedded_contract();
        assert!(!contract.cloud_account_required);
        assert!(!contract.subscription_required);
        assert!(!contract.activation_required);
        assert!(!contract.license_server_required);
        assert_eq!(contract.worker_limit, 0);
        assert_eq!(contract.rigos_dev_fee_percent, 0);
        assert!(!contract.forced_pool);
    }

    #[test]
    fn usb_is_runtime_and_internal_disks_are_never_targets() {
        let contract = embedded_contract();
        assert_eq!(contract.runtime_medium, "usb");
        assert_eq!(contract.architecture, "x86_64");
        assert!(contract.legacy_bios && contract.uefi);
        assert!(!contract.internal_disk_install);
        assert!(!contract.internal_disk_auto_mount);
        assert!(!contract.internal_disk_auto_format);
        assert!(!contract.internal_disk_swap);
        assert_eq!(contract.usb.root_slots, ["RIGOS_ROOT_A", "RIGOS_ROOT_B"]);
        assert_eq!(contract.usb.state_partition, "RIGOS_STATE");
        assert!(contract.usb.root_read_only);
        assert!(contract.usb.zram_swap);
        assert!(contract.usb.bounded_journal && contract.usb.bounded_event_log);
        assert!(contract.usb.atomic_policy_writes && contract.usb.checksummed_policy_revisions);
    }
}
