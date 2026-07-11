use std::path::PathBuf;
use std::process::Command;

fn helper_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/usr/lib/rigos/rigos-admin-password")
}

#[test]
fn admin_password_draw_stays_inside_80_and_40_column_ptys() {
    let fixture = r#"
import errno
import fcntl
import os
import pty
import runpy
import struct
import sys
import termios
import tty

source = sys.argv[1]
namespace = runpy.run_path(source, run_name="rigos_admin_password_tty_test")
draw = namespace["draw"]

for columns in (80, 40):
    master, slave = pty.openpty()

    try:
        fcntl.ioctl(
            slave,
            termios.TIOCSWINSZ,
            struct.pack("HHHH", 25, columns, 0, 0),
        )

        tty.setcbreak(slave)

        stream = os.fdopen(
            os.dup(slave),
            "w",
            encoding="utf-8",
            buffering=1,
        )

        draw(
            stream,
            bytearray(b"secret"),
            bytearray(b"secret"),
            False,
            0,
            "Password confirmation does not match.",
        )

        stream.close()
        os.close(slave)
        slave = -1

        chunks = []

        while True:
            try:
                chunk = os.read(master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise

            if not chunk:
                break

            chunks.append(chunk)

        payload = b"".join(chunks)
        prefix = b"\x1b[2J\x1b[H"

        assert payload.startswith(prefix), payload[:32]
        assert b"\r\r\n" not in payload
        assert b"\n" not in payload.replace(b"\r\n", b"")

        body = payload[len(prefix):]
        assert body.endswith(b"\r\n")

        lines = body[:-2].split(b"\r\n")
        expected_width = min(64, columns - 1)

        assert len(lines) <= 25
        assert all(len(line) == expected_width for line in lines), (
            columns,
            expected_width,
            [len(line) for line in lines],
        )
        assert any(b"RIGOS ADMINISTRATOR PASSWORD" in line for line in lines)
        assert any(b"TAB next" in line for line in lines)
        assert any(b"ENTER select" in line for line in lines)
    finally:
        if slave >= 0:
            os.close(slave)
        os.close(master)
"#;

    let result = Command::new("python3")
        .arg("-c")
        .arg(fixture)
        .arg(helper_path())
        .status()
        .expect("run administrator password PTY fixture");

    assert!(
        result.success(),
        "administrator password PTY geometry fixture failed"
    );
}

#[test]
fn admin_password_terminal_mode_preserves_output_processing() {
    let helper =
        std::fs::read_to_string(helper_path()).expect("read administrator password helper");

    assert!(
        helper.contains("tty.setcbreak(fd)"),
        "password helper must use cbreak input mode"
    );

    assert!(
        !helper.contains("tty.setraw(fd)"),
        "password helper must not disable terminal output processing"
    );

    assert!(
        helper.contains("console_columns - 1"),
        "password helper must reserve the automatic-wrap column"
    );

    assert!(
        helper.contains("termios.tcsetattr(fd, termios.TCSADRAIN, old)"),
        "password helper must restore the original terminal mode"
    );
}
