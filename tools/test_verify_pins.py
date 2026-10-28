"""Live pin consistency and negative controls for all product Cargo graphs."""
import importlib.util
import re
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("rnx_verify", Path(__file__).with_name("verify.py"))
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
REVISION = "1" * 40
OLD = "2" * 40
URL = "https://github.com/nilket-com/rune"
LOCKS = (
    "Cargo.lock", "tools/project/Cargo.lock", "adapters/candle/Cargo.lock",
    "adapters/polars/Cargo.lock", "adapters/postgres/Cargo.lock",
    "demos/candle/Cargo.lock", "demos/polars/Cargo.lock", "servers/http-postgres/Cargo.lock",
)


class PinConsistency(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for relative in ("Cargo.toml", "tools/project/Cargo.toml"):
            self.write(relative, f'[dependencies]\nrune = {{ git = "{URL}", rev = "{REVISION}" }}\n')
        for relative in LOCKS:
            self.write(relative, self.lock())
        for relative in ("src/lib.rs", "jupyter/src/worker.rs"):
            self.write(relative, f'const RUNE_VERSION: &str = "0.15.0 (git {REVISION})";\n')

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def lock(self):
        return "".join(f'[[package]]\nname = "{name}"\nversion = "0.15.0"\n'
                       f'source = "git+{URL}?rev={REVISION}#{REVISION}"\n'
                       for name in ("rune", "rune-macros"))

    def test_checked_in_product_pins_agree(self):
        result = VERIFY.check_rune_pins(Path(__file__).resolve().parents[1])
        self.assertTrue(set(LOCKS).issubset(result["locks"]))
        self.assertEqual(len(result["versions"]), 2)

    def test_every_product_lock_rejects_a_stale_revision(self):
        for relative in LOCKS:
            with self.subTest(lock=relative):
                self.write(relative, self.lock().replace(REVISION, OLD))
                with self.assertRaisesRegex(ValueError, re.escape(relative)):
                    VERIFY.check_rune_pins(self.root)
                self.write(relative, self.lock())

    def test_every_rune_package_and_both_source_revision_fields_are_checked(self):
        for stale in (
            self.lock().replace(f"#{REVISION}", f"#{OLD}", 1),
            self.lock().replace(f"rev={REVISION}", f"rev={OLD}", 1),
            self.lock().replace(f"#{REVISION}", f"#{OLD}"),
            self.lock().replace(f"git+{URL}?rev={REVISION}#{REVISION}", "registry+old", 1),
            self.lock().rsplit(REVISION, 1)[0] + OLD + self.lock().rsplit(REVISION, 1)[1],
        ):
            self.write("Cargo.lock", stale)
            with self.assertRaises(ValueError):
                VERIFY.check_rune_pins(self.root)

    def test_new_product_lock_is_discovered(self):
        self.write("adapters/new/Cargo.lock", self.lock().replace(REVISION, OLD))
        with self.assertRaisesRegex(ValueError, "adapters/new/Cargo.lock"):
            VERIFY.check_rune_pins(self.root)

    def test_manifest_pin_and_both_reported_versions_are_checked(self):
        for relative in ("tools/project/Cargo.toml", "src/lib.rs", "jupyter/src/worker.rs"):
            path = self.root / relative
            original = path.read_text()
            path.write_text(original.replace(REVISION, OLD))
            with self.assertRaisesRegex(ValueError, re.escape(relative)):
                VERIFY.check_rune_pins(self.root)
            path.write_text(original)

    def test_historical_probes_and_build_outputs_are_not_product_graphs(self):
        for relative in ("probes/old/Cargo.lock", "target/old/Cargo.lock"):
            self.write(relative, self.lock().replace(REVISION, OLD))
        result = VERIFY.check_rune_pins(self.root)
        self.assertEqual(set(result["locks"]), set(LOCKS))


if __name__ == "__main__":
    unittest.main()
