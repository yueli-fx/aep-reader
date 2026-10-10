"""Exercise the standalone Cargo consumer with synthetic inputs and optional AEPs.

Only the Python standard library is required. Artifacts remain in the specified
task directory for review; remove them after checks complete. Inputs are not copied.
"""
import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import time

def chunk(tag, data):
    return tag + struct.pack(">I", len(data)) + data + (b"\0" if len(data) % 2 else b"")

def container(tag, form, children):
    return chunk(tag, form + b"".join(children))

def group(form, children):
    return container(b"LIST", form, children)

def fixture():
    descriptor = bytearray(124); descriptor[3] = 1; descriptor[0x3b] = 8; descriptor[0x44] = 1
    header = bytearray(20); struct.pack_into(">I", header, 8, 1); struct.pack_into(">I", header, 16, 48)
    key = struct.pack(">IBBBB5d", 4000, 2, 3, 0, 0, 80, 2, .25, 3, .5)
    scale = bytearray(124); scale[3] = 3; scale[0x3b] = 8
    props = [
        chunk(b"tdmn", b"ADBE Opacity"), group(b"tdbs", [chunk(b"tdb4", descriptor), group(b"list", [chunk(b"lhd3", header), chunk(b"ldat", key)])]),
        chunk(b"tdmn", b"ADBE Scale"), group(b"tdbs", [chunk(b"tdb4", scale), chunk(b"cdat", struct.pack(">3d", 1, 1, 1))]),
        chunk(b"tdmn", b"Custom Unreadable"), chunk(b"xxxx", b"unknown payload"),
    ]
    comp = bytearray(204); struct.pack_into(">I", comp, 8, 8000); struct.pack_into(">2H", comp, 0x8c, 320, 240); struct.pack_into(">I", comp, 0x9c, 24 * 65536)
    item = bytearray(64); struct.pack_into(">H", item, 0, 4); struct.pack_into(">I", item, 16, 3)
    layer = bytearray(164); struct.pack_into(">I", layer, 0, 7); layer[0x27] = 1
    return container(b"RIFX", b"Egg!", [group(b"Fold", [group(b"Item", [chunk(b"idta", item), chunk(b"Utf8", "Demo 月离".encode()), chunk(b"cdta", comp), group(b"Layr", [chunk(b"ldta", layer), chunk(b"Utf8", b"Layer"), group(b"tdgp", props)])])])])

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", default="stable")
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--fixture", type=Path, action="append", default=[])
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    artifact = (args.artifact_dir or repo / ".task" / f"consumer-{time.time_ns()}").resolve()
    if repo / ".task" not in artifact.parents:
        raise ValueError("artifacts must be inside this repository's .task directory")
    artifact.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy(); env.setdefault("CARGO_TARGET_DIR", str(artifact / "target"))
    env["CARGO_TERM_COLOR"] = "never"
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    results = []; commands = []
    def run(cmd, successful=True):
        result = subprocess.run(cmd, cwd=repo, env=env, text=True, encoding="utf-8", capture_output=True, creationflags=flags, timeout=300)
        commands.append({"command": [str(a) for a in cmd], "exit_code": result.returncode})
        (artifact / f"command-{len(commands)}.log").write_text(result.stdout + result.stderr, encoding="utf-8")
        if successful and result.returncode:
            raise RuntimeError(f"command failed: {cmd}\n{result.stdout}\n{result.stderr}")
        return result
    def check(name, condition):
        results.append({"check": name, "passed": bool(condition)})
        if not condition: raise AssertionError(name)
    cargo = ["cargo", f"+{args.toolchain}"]
    manifest = repo / "examples/consumer/Cargo.toml"
    build = ["--locked", "--manifest-path", str(manifest)] + (["--offline"] if args.offline else [])
    good = artifact / "consumer.aep"; good.write_bytes(fixture())
    bad = artifact / "malformed.aep"; bad.write_bytes(b"not an AEP")
    report = {"toolchain": args.toolchain, "checks": results, "commands": commands}
    try:
        run(cargo + ["build"] + build + ["--no-default-features"])
        tree = run(cargo + ["tree"] + build + ["--no-default-features", "--edges", "normal", "--prefix", "none"]).stdout.splitlines()
        check("default consumer has only itself and aep-reader", len(tree) == 2 and tree[0].startswith("aep-reader-consumer ") and tree[1].startswith("aep-reader "))
        binary = Path(env["CARGO_TARGET_DIR"]) / "debug" / ("aep-reader-consumer.exe" if os.name == "nt" else "aep-reader-consumer")
        help_result = run([str(binary), "--help"])
        check("help is successful", "usage:" in help_result.stdout)
        missing = run([str(binary)], False)
        check("missing input fails with usage", missing.returncode != 0 and "usage:" in missing.stderr)
        text = run([str(binary), str(good)])
        check("public model summary and Unicode names", "project: 1 compositions, 1 layers, 0 footage items" in text.stdout and "Demo 月离" in text.stdout)
        check("property filter and keyframe values", "ticks=4000, seconds=0.5, value=[80], interpolation=2/3" in text.stdout)
        check("partial decoding remains usable and diagnostic", "incomplete nodes: 1" in text.stderr and "unsupported-property at byte" in text.stderr)
        scale = run([str(binary), str(good), "--property", "ADBE Scale"])
        check("static property and known baseline", "static: [1, 1, 1]" in scale.stdout and "modified=Some(false)" in scale.stdout)
        absent = run([str(binary), str(good), "--property", "Absent"])
        check("missing property is explicit", "Absent: <absent>" in absent.stdout)
        over = run([str(binary), str(good), "--max-bytes", "8"], False)
        check("bounded input reports resource limit", over.returncode != 0 and "input bytes limit exceeded: 9 > 8" in over.stderr)
        malformed = run([str(binary), str(bad)], False)
        check("malformed input reports parse error", malformed.returncode != 0 and "invalid AEP at byte" in malformed.stderr)
        invalid_limit = run([str(binary), str(good), "--max-bytes", "0"], False)
        check("zero limit is rejected", invalid_limit.returncode != 0 and "must be positive" in invalid_limit.stderr)
        disabled = run([str(binary), str(good), "--json"], False)
        check("optional JSON is explicitly gated", disabled.returncode != 0 and "--features json" in disabled.stderr)
        run(cargo + ["build"] + build + ["--features", "json"])
        serialized = run([str(binary), str(good), "--json"])
        model = json.loads(serialized.stdout)
        check("JSON stdout is clean and preserves decoded model", model["compositions"][0]["id"] == 3 and model["compositions"][0]["layers"][0]["properties"][0]["property"]["keyframes"][0]["value"]["Numbers"][0]["value"] == 80 and "unsupported-property at byte" in serialized.stderr)
        for path in args.fixture:
            actual = run([str(binary), str(path.resolve()), "--json"])
            model = json.loads(actual.stdout)
            check(f"real AEP: {path.name}", bool(model["compositions"]) and "diagnostics:" in actual.stderr)
    finally:
        (artifact / "consumer-check.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"toolchain": args.toolchain, "checks_passed": len(results), "artifact_dir": str(artifact)}, ensure_ascii=False))

if __name__ == "__main__": main()
