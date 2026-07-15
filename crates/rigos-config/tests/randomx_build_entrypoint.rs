use std::fs;
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn performance_entrypoint_uses_exact_lf_git_version_authority() {
    let attributes = fs::read_to_string(repo_path(".gitattributes")).unwrap();
    let entrypoint =
        fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh")).unwrap();
    let image_builder = fs::read_to_string(repo_path("scripts/build-usb-image.sh")).unwrap();
    let image_verifier =
        fs::read_to_string(repo_path("scripts/verify-randomx-performance-image.sh")).unwrap();
    let image_hook = fs::read_to_string(repo_path("build/usb/hooks/010-rigos.chroot")).unwrap();

    assert!(
        attributes
            .lines()
            .any(|line| line == "build/usb/version.env text eol=lf")
    );
    assert!(entrypoint.contains(
        "git -c safe.directory=\"$repo\" show HEAD:build/usb/version.env >\"$version_env\""
    ));
    assert!(entrypoint.contains("if grep -q $'\\r' \"$version_env\"; then"));
    assert!(entrypoint.contains("source \"$version_env\""));
    assert!(!entrypoint.contains("source ./build/usb/version.env"));
    assert!(entrypoint.contains("rigos-randomx-msr"));
    assert!(entrypoint.contains("rigos-miner-gate"));
    assert!(entrypoint.contains("--test randomx_build_entrypoint"));

    assert!(image_builder.contains("created_partition_nodes=()"));
    assert!(image_builder.contains(r#"if [[ ! -e "$node" ]]; then"#));
    assert!(
        image_builder
            .contains(r#"[[ -b "$node" ]] || die "partition node is not a block device: $node""#)
    );
    assert!(image_builder.contains(r#"stat -c '%t:%T' "$node""#));
    assert!(image_builder.contains("partition node device mismatch"));
    assert!(image_builder.contains(r#"rm -f -- "${created_partition_nodes[@]}""#));
    assert!(
        !image_builder
            .contains(r#"[[ ! -e "$node" ]] || die "partition node already exists: $node""#)
    );

    assert!(image_builder.contains(r#"bios_root_loop="""#));
    assert!(image_builder.contains(r#"bios_efi_loop="""#));
    assert!(image_builder.contains(r#"blockdev --getss "$loop""#));
    assert!(image_builder.contains(r#"/sys/class/block/${loop_name}p1/start"#));
    assert!(image_builder.contains(r#"/sys/class/block/${loop_name}p2/start"#));
    assert!(image_builder.contains(r#"--offset "$root_offset_bytes""#));
    assert!(image_builder.contains(r#"--sizelimit "$root_size_bytes""#));
    assert!(image_builder.contains(r#"--offset "$efi_offset_bytes""#));
    assert!(image_builder.contains(r#"--sizelimit "$efi_size_bytes""#));
    assert!(image_builder.contains(r#"mount "$bios_root_loop" "$work/mnt/a""#));
    assert!(image_builder.contains(r#"mount "$bios_efi_loop" "$work/mnt/efi""#));
    assert!(image_builder.contains(r#"printf '(hd0) %s\n(hd1) %s\n(hd2) %s\n'"#));
    assert!(image_builder.contains(r#"losetup -d "$bios_root_loop""#));
    assert!(image_builder.contains(r#"losetup -d "$bios_efi_loop""#));
    assert!(image_builder.contains("runtime loop path leaked into BIOS GRUB load configuration"));
    assert!(!image_builder.contains(r#"mount "$p1" "$work/mnt/efi"; mount "$p2" "$work/mnt/a""#));

    let root_loop_create = image_builder.find(r#"bios_root_loop="$("#).unwrap();
    let efi_loop_create = image_builder.find(r#"bios_efi_loop="$("#).unwrap();
    let root_mount = image_builder
        .find(r#"mount "$bios_root_loop" "$work/mnt/a""#)
        .unwrap();
    let efi_mount = image_builder
        .find(r#"mount "$bios_efi_loop" "$work/mnt/efi""#)
        .unwrap();
    let device_map_write = image_builder
        .find(r#"printf '(hd0) %s\n(hd1) %s\n(hd2) %s\n'"#)
        .unwrap();
    let bios_install = image_builder.find("grub-install --target=i386-pc").unwrap();
    let efi_install = image_builder
        .find("grub-install --target=x86_64-efi")
        .unwrap();
    let device_map_remove = image_builder
        .rfind(r#"rm -f -- "$bios_device_map""#)
        .unwrap();
    let grub_copy = image_builder
        .find(r#"cp -a "$work/mnt/a/boot/grub/." "$work/mnt/b/boot/grub/""#)
        .unwrap();

    assert!(root_loop_create < root_mount);
    assert!(efi_loop_create < efi_mount);
    assert!(root_mount < device_map_write);
    assert!(efi_mount < device_map_write);
    assert!(device_map_write < bios_install);
    assert!(bios_install < efi_install);
    assert!(efi_install < device_map_remove);
    assert!(device_map_remove < grub_copy);
    assert!(!image_builder.contains("--recheck"));

    assert!(image_hook.contains("/usr/lib/rigos/rigos-randomx-msr"));
    assert!(image_hook.contains("rigos-randomx-msr.service rigos-miner.service"));

    assert!(image_verifier.contains("msr_support=\"module\""));
    assert!(image_verifier.contains("msr_support=\"builtin\""));
    assert!(image_verifier.contains("modules.builtin"));
    assert!(image_verifier.contains("kernel/arch/x86/kernel/msr\\.ko"));
    assert!(image_verifier.contains("Do not use grep -q in a pipe while pipefail is enabled"));
    assert!(!image_verifier.contains("grep -Eq"));
    assert!(
        image_verifier
            .contains("kernel MSR support is absent from module files and modules.builtin")
    );
}
