use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::process::{Command, Output};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn bios_grub_bootstrap_and_artifact_names_are_locked() {
    let entrypoint =
        fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh")).unwrap();
    let image_builder = fs::read_to_string(repo_path("scripts/build-usb-image.sh")).unwrap();
    let wrapper = fs::read_to_string(repo_path("scripts/rigos-grub-install-wrapper.sh")).unwrap();

    assert!(entrypoint.contains("real_grub_install=\"$(command -v grub-install)\""));
    assert!(entrypoint.contains("./scripts/rigos-grub-install-wrapper.sh"));
    assert!(entrypoint.contains("export RIGOS_REAL_GRUB_INSTALL=\"$real_grub_install\""));
    assert!(entrypoint.contains("export PATH=\"$grub_wrapper_dir:$PATH\""));
    assert!(entrypoint.contains("--test bios_grub_bootstrap"));

    assert!(
        wrapper.contains("bios_modules='part_msdos ext2 search search_fs_uuid normal configfile'")
    );
    assert!(wrapper.contains("\"--modules=$bios_modules\""));
    assert!(wrapper.contains("if [[ \"$target\" == \"i386-pc\" ]]; then"));
    assert!(wrapper.contains("caller supplied a conflicting BIOS module list"));
    assert!(wrapper.contains("RIGOS BIOS GRUB embedded modules:"));

    assert!(image_builder.contains(r#"image_name="rigos-usb-amd64-${RIGOS_IMAGE_VERSION}.img""#));
    assert!(
        image_builder
            .contains(r#"recovery_name="rigos-recovery-amd64-${RIGOS_IMAGE_VERSION}.iso""#)
    );
}

#[cfg(unix)]
struct TemporaryDirectory {
    path: PathBuf,
}

#[cfg(unix)]
impl TemporaryDirectory {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rigos-bios-grub-bootstrap-{}-{timestamp}",
            std::process::id()
        ));

        fs::create_dir(&path).unwrap();
        Self { path }
    }
}

#[cfg(unix)]
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(unix)]
fn write_fake_grub_install(path: &Path) {
    fs::write(
        path,
        "#!/bin/sh\nset -eu\n: \"${RIGOS_GRUB_TEST_CAPTURE:?}\"\nprintf '%s\\n' \"$@\" >\"$RIGOS_GRUB_TEST_CAPTURE\"\n",
    )
    .unwrap();

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn run_wrapper(wrapper: &Path, real: &Path, capture: &Path, arguments: &[&str]) -> Output {
    Command::new("/bin/bash")
        .arg(wrapper)
        .args(arguments)
        .env("RIGOS_REAL_GRUB_INSTALL", real)
        .env("RIGOS_GRUB_TEST_CAPTURE", capture)
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn bios_grub_wrapper_injects_modules_only_for_i386_pc() {
    let temporary = TemporaryDirectory::new();
    let real = temporary.path.join("real-grub-install");
    let capture = temporary.path.join("arguments.txt");
    let wrapper = repo_path("scripts/rigos-grub-install-wrapper.sh");

    write_fake_grub_install(&real);

    let bios = run_wrapper(
        &wrapper,
        &real,
        &capture,
        &[
            "--target=i386-pc",
            "--boot-directory=/tmp/boot",
            "--no-floppy",
            "/dev/loop0",
        ],
    );

    assert!(
        bios.status.success(),
        "{}",
        String::from_utf8_lossy(&bios.stderr)
    );
    assert_eq!(
        fs::read_to_string(&capture).unwrap(),
        "--modules=part_msdos ext2 search search_fs_uuid normal configfile\n\
--target=i386-pc\n\
--boot-directory=/tmp/boot\n\
--no-floppy\n\
/dev/loop0\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&bios.stdout),
        "RIGOS BIOS GRUB embedded modules: part_msdos ext2 search search_fs_uuid normal configfile\n"
    );

    let uefi = run_wrapper(
        &wrapper,
        &real,
        &capture,
        &["--target=x86_64-efi", "--removable", "--no-nvram"],
    );

    assert!(
        uefi.status.success(),
        "{}",
        String::from_utf8_lossy(&uefi.stderr)
    );
    assert_eq!(
        fs::read_to_string(&capture).unwrap(),
        "--target=x86_64-efi\n--removable\n--no-nvram\n"
    );
    assert!(uefi.stdout.is_empty());

    fs::remove_file(&capture).unwrap();

    let conflict = run_wrapper(
        &wrapper,
        &real,
        &capture,
        &["--target=i386-pc", "--modules=normal", "/dev/loop0"],
    );

    assert!(!conflict.status.success());
    assert!(
        String::from_utf8_lossy(&conflict.stderr)
            .contains("caller supplied a conflicting BIOS module list")
    );
    assert!(!capture.exists());
}
