"""Check a Cargo source archive without extracting it or requiring dependencies."""
import argparse
from pathlib import PurePosixPath
import tarfile


ROOT_FILES = {
    "Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json",
    "README.md", "READING-COVERAGE.md", "VALIDATION.md", "PROVENANCE.md",
    "COMPARISON.md", "CAPABILITIES.md", "CHANGELOG.md", "LICENSE-MIT", "LICENSE-APACHE",
}


def check_package(path):
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        names = []
        for entry in entries:
            parts = PurePosixPath(entry.name).parts
            if (not entry.isfile() or len(parts) < 2 or parts[0] != "aep-reader-0.1.0"
                    or any(part in {"", ".", ".."} for part in parts)):
                raise ValueError(f"unexpected archive entry: {entry.name}")
            relative = PurePosixPath(*parts[1:])
            allowed = (
                len(parts) == 2 and (relative.name in ROOT_FILES)
                or parts[1] in {"src", "tests"} and relative.suffix == ".rs"
                or parts[1] == "examples" and len(parts) == 3 and relative.suffix == ".rs"
            )
            if not allowed or relative.is_absolute():
                raise ValueError(f"file outside the source-package boundary: {entry.name}")
            names.append(str(relative))
        if len(set(names)) != len(names):
            raise ValueError("duplicate source-package entry")
        if not {"Cargo.toml", "src/lib.rs", "tests/reading_closure.rs", "LICENSE-MIT", "LICENSE-APACHE"}.issubset(names):
            raise ValueError("required source or license missing")
        return names


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("crate")
    args = parser.parse_args()
    names = check_package(args.crate)
    print(f"source package: {len(names)} allowed files; no fixtures, caches or toolchains")
