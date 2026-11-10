#!/usr/bin/env python3
"""Unit tests for Bella data-directory migration and leftover env names."""
import importlib.util, os, pathlib, shutil, tempfile, unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("bella_launch", ROOT / "scripts" / "launch.py")
LAUNCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LAUNCH)


class LaunchPaths(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bella-paths-")
        self.root = pathlib.Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_env_first_prefers_bella_and_treats_empty_as_unset(self):
        previous = {name: os.environ.get(name) for name in ("BELLA_DATA", "TERM_SOCIAL_DATA")}
        try:
            os.environ["BELLA_DATA"] = "/tmp/bella-data"
            os.environ["TERM_SOCIAL_DATA"] = "/tmp/legacy-data"
            self.assertEqual(LAUNCH.env_first("BELLA_DATA", "TERM_SOCIAL_DATA"), "/tmp/bella-data")
            os.environ["BELLA_DATA"] = ""
            self.assertEqual(LAUNCH.env_first("BELLA_DATA", "TERM_SOCIAL_DATA"), "/tmp/legacy-data")
            os.environ["TERM_SOCIAL_DATA"] = ""
            self.assertEqual(LAUNCH.env_first("BELLA_DATA", "TERM_SOCIAL_DATA", default="fallback"), "fallback")
        finally:
            for name, value in previous.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value

    def test_rename_moves_legacy_directory_when_the_new_one_is_missing(self):
        old, new = self.root / "term-social", self.root / "bella"
        old.mkdir()
        (old / "chat.sqlite").write_text("data")
        result = LAUNCH.migrate_legacy_dir(new, old)
        self.assertEqual(result, new)
        self.assertTrue((new / "chat.sqlite").is_file())
        self.assertEqual((new / "chat.sqlite").read_text(), "data")
        self.assertFalse(old.exists())

    def test_existing_new_directory_is_left_alone(self):
        old, new = self.root / "term-social", self.root / "bella"
        old.mkdir(); new.mkdir()
        (old / "legacy").write_text("old")
        (new / "current").write_text("new")
        result = LAUNCH.migrate_legacy_dir(new, old)
        self.assertEqual(result, new)
        self.assertEqual((old / "legacy").read_text(), "old")
        self.assertEqual((new / "current").read_text(), "new")

    def test_copy_fallback_leaves_the_old_directory_in_place(self):
        old, new = self.root / "term-social", self.root / "bella"
        old.mkdir()
        (old / "chat.sqlite").write_text("copied")
        original = LAUNCH.os.rename

        def fail_rename(src, dst):
            raise OSError(18, "Invalid cross-device link")

        LAUNCH.os.rename = fail_rename
        try:
            result = LAUNCH.migrate_legacy_dir(new, old)
        finally:
            LAUNCH.os.rename = original
        self.assertEqual(result, new)
        self.assertEqual((new / "chat.sqlite").read_text(), "copied")
        self.assertEqual((old / "chat.sqlite").read_text(), "copied")

    def test_settings_workspace_inside_the_old_directory_is_rewritten(self):
        old, new = self.root / "term-social", self.root / "bella"
        old.mkdir()
        (old / "workspace").mkdir()
        (old / "settings.json").write_text(
            '{"name":"migrated-user","endpoint":"http://127.0.0.1:9/v1","model":"fixture","workspace":"%s"}\n'
            % (old / "workspace")
        )
        result = LAUNCH.migrate_legacy_dir(new, old)
        self.assertEqual(result, new)
        data = __import__("json").loads((new / "settings.json").read_text())
        self.assertEqual(data["workspace"], str((new / "workspace").resolve()))
        self.assertEqual(data["name"], "migrated-user")

    def test_settings_workspace_outside_the_data_directory_is_left_alone(self):
        old, new = self.root / "term-social", self.root / "bella"
        external = self.root / "elsewhere"
        external.mkdir()
        old.mkdir()
        (old / "settings.json").write_text(
            '{"name":"migrated-user","endpoint":"http://127.0.0.1:9/v1","model":"fixture","workspace":"%s"}\n'
            % external
        )
        LAUNCH.migrate_legacy_dir(new, old)
        data = __import__("json").loads((new / "settings.json").read_text())
        self.assertEqual(data["workspace"], str(external))

    def test_failed_copy_returns_old_directory_and_does_not_delete_it(self):
        old, new = self.root / "term-social", self.root / "bella"
        old.mkdir()
        (old / "chat.sqlite").write_text("keep-me")
        original_rename = LAUNCH.os.rename
        original_copy = LAUNCH.shutil.copytree

        def fail_rename(src, dst):
            raise OSError(18, "Invalid cross-device link")

        def fail_copy(*args, **kwargs):
            raise OSError("copy failed")

        LAUNCH.os.rename = fail_rename
        LAUNCH.shutil.copytree = fail_copy
        try:
            result = LAUNCH.migrate_legacy_dir(new, old)
        finally:
            LAUNCH.os.rename = original_rename
            LAUNCH.shutil.copytree = original_copy
        self.assertEqual(result, old)
        self.assertTrue((old / "chat.sqlite").is_file())
        self.assertEqual((old / "chat.sqlite").read_text(), "keep-me")
        self.assertFalse(new.exists())


if __name__ == "__main__":
    unittest.main()
