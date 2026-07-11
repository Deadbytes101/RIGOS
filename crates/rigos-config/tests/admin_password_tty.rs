use std::path::PathBuf;
use std::process::Command;

fn helper_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/usr/lib/rigos/rigos-admin-password")
}

#[test]
fn admin_password_draw_centers_adaptive_panels_inside_common_ptys() {
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

def expected_geometry(rows, columns):
    draw_columns = max(20, max(columns, 24) - 1)
    panel_width = int(draw_columns * 0.76)
    panel_width = max(58, panel_width)
    panel_width = min(panel_width, draw_columns - 2)
    panel_width = max(20, panel_width)
    panel_height = int(max(rows, 12) * 0.56)
    panel_height = max(18, panel_height)
    panel_height = min(panel_height, max(rows, 12) - 2)
    panel_height = max(10, panel_height)
    left = max(0, (draw_columns - panel_width) // 2)
    top = max(0, (max(rows, 12) - panel_height) // 2)
    return draw_columns, panel_width, panel_height, left, top

for rows, columns in ((25, 80), (30, 100), (43, 132), (60, 180)):
    master, slave = pty.openpty()

    try:
        fcntl.ioctl(
            slave,
            termios.TIOCSWINSZ,
            struct.pack("HHHH", rows, columns, 0, 0),
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
        draw_columns, expected_width, expected_height, expected_left, expected_top = (
            expected_geometry(rows, columns)
        )
        blank_prefix = 0
        for line in lines:
            if line:
                break
            blank_prefix += 1
        panel_lines = lines[blank_prefix:]

        assert blank_prefix == expected_top, (
            rows,
            columns,
            expected_top,
            blank_prefix,
        )
        assert len(panel_lines) == expected_height, (
            rows,
            columns,
            expected_height,
            len(panel_lines),
        )
        assert all(len(line) <= draw_columns for line in panel_lines), (
            rows,
            columns,
            draw_columns,
            [len(line) for line in panel_lines],
        )
        assert all(line.startswith(b" " * expected_left) for line in panel_lines), (
            rows,
            columns,
            expected_left,
            panel_lines[:2],
        )
        assert all(len(line[expected_left:]) == expected_width for line in panel_lines), (
            rows,
            columns,
            expected_width,
            [len(line[expected_left:]) for line in panel_lines],
        )
        assert any(b"RIGOS // DEADBYTE LOCAL CONSOLE" in line for line in panel_lines)
        assert any(b"ADMIN PASSWORD AUTHORITY" in line for line in panel_lines)
        assert any(b"TAB NEXT" in line for line in panel_lines)
        assert any(b"ENTER SELECT" in line for line in panel_lines)
        assert any(b"SPACE SHOW/HIDE" in line for line in panel_lines)
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
        helper.contains("FALLBACK_ROWS") && helper.contains("HEIGHT_FRACTION"),
        "password helper must size the panel from rows as well as columns"
    );

    assert!(
        helper.contains("apply_console_font()")
            && helper.contains("RIGOS_ADMIN_PASSWORD_SKIP_SETFONT")
            && helper.contains("/usr/bin/setfont")
            && helper.contains("TerminusBold20x10"),
        "password helper must try a larger setup console font without making it mandatory"
    );

    assert!(
        helper.contains("termios.tcsetattr(fd, termios.TCSADRAIN, old)"),
        "password helper must restore the original terminal mode"
    );
}
