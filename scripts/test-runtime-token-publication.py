#!/usr/bin/env python3
import importlib.machinery
import importlib.util
import json
import os
import shutil
import stat
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render"
PUBLISHER = ROOT / "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-publish"


def load_source(name: str, path: Path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    if spec is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class Environment:
    def __init__(self, **values: str):
        self.values = values
        self.original: dict[str, str | None] = {}

    def __enter__(self):
        for key, value in self.values.items():
            self.original[key] = os.environ.get(key)
            os.environ[key] = value
        return self

    def __exit__(self, _type, _value, _traceback):
        for key, value in self.original.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def create_state(root: Path) -> Path:
    state = root / "state"
    revision = state / "revisions/r1"
    (revision / "flight-sheets").mkdir(parents=True)
    (revision / "policy.json").write_text(
        json.dumps({
            "schema": "rigos.policy/v1",
            "active_flight_sheet": "sheet",
        }),
        encoding="utf-8",
    )
    (revision / "flight-sheets/sheet.json").write_text(
        json.dumps({
            "schema": "rigos.flight-sheet/v1",
            "backend": "xmrig",
            "algorithm": "rx/0",
            "cpu": {"threads": "auto"},
        }),
        encoding="utf-8",
    )
    (revision / "xmrig.json").write_text(
        json.dumps({
            "autosave": False,
            "cpu": {"enabled": True, "huge-pages": True},
            "pools": [{
                "url": "pool.example:1234",
                "user": "secret-wallet",
                "pass": "worker",
                "algo": "rx/0",
            }],
        }),
        encoding="utf-8",
    )
    (state / "current").symlink_to("revisions/r1")
    return state


class RuntimeTokenPublicationTests(unittest.TestCase):
    def test_token_survives_render_stage_cleanup_and_is_reused(self):
        with tempfile.TemporaryDirectory(prefix="rigos-token-publication-") as temporary:
            root = Path(temporary)
            state = create_state(root)
            authority_runtime = root / "runtime"
            token_path = authority_runtime / "xmrig-api-token"
            first_stage = authority_runtime / ".render-stage.first"
            first_stage.mkdir(parents=True)

            with Environment(
                RIGOS_STATE_PATH=str(state),
                RIGOS_RUNTIME_PATH=str(first_stage),
                RIGOS_XMRIG_API_TOKEN_PATH=str(token_path),
                RIGOS_RENDER_SKIP_CHOWN="1",
            ):
                first = load_source("rigos_runtime_render_publication_first", RENDERER)
                self.assertEqual(first.API_TOKEN, token_path)
                self.assertEqual(first.main(), 0)

            self.assertTrue(token_path.is_file())
            self.assertEqual(stat.S_IMODE(token_path.stat().st_mode), 0o600)
            self.assertFalse((first_stage / "xmrig-api-token").exists())
            token_first = token_path.read_text(encoding="ascii").strip()
            private_first = json.loads((first_stage / "xmrig.json").read_text(encoding="utf-8"))
            status_first = json.loads(
                (first_stage / "runtime-config-status.json").read_text(encoding="utf-8")
            )
            self.assertEqual(private_first["http"]["access-token"], token_first)
            self.assertEqual(status_first["http_api"]["token_path"], str(token_path))

            shutil.rmtree(first_stage)
            self.assertTrue(token_path.is_file())

            second_stage = authority_runtime / ".render-stage.second"
            second_stage.mkdir()
            with Environment(
                RIGOS_STATE_PATH=str(state),
                RIGOS_RUNTIME_PATH=str(second_stage),
                RIGOS_XMRIG_API_TOKEN_PATH=str(token_path),
                RIGOS_RENDER_SKIP_CHOWN="1",
            ):
                second = load_source("rigos_runtime_render_publication_second", RENDERER)
                self.assertEqual(second.main(), 0)

            private_second = json.loads((second_stage / "xmrig.json").read_text(encoding="utf-8"))
            self.assertEqual(private_second["http"]["access-token"], token_first)
            self.assertEqual(token_path.read_text(encoding="ascii").strip(), token_first)
            shutil.rmtree(second_stage)
            self.assertTrue(token_path.is_file())

    def test_publisher_wires_authority_token_outside_stage(self):
        publisher = PUBLISHER.read_text(encoding="utf-8")
        self.assertIn('RIGOS_RUNTIME_PATH="$stage" \\\n', publisher)
        self.assertIn(
            'RIGOS_XMRIG_API_TOKEN_PATH="$runtime/xmrig-api-token" \\\n',
            publisher,
        )
        self.assertIn('    "$renderer"\n', publisher)


if __name__ == "__main__":
    unittest.main(verbosity=2)
