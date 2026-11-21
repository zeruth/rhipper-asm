"""Builds the machine coverage report: measures the workspace (the roadmap's cards, the crates'
lines and tests, the lifts' instruction forms, the consoles' platform runtimes, the committed
fixtures, the commits) and writes
the measurements into the page `index.html` beside this script, as `<out>/index.html`.

The levels of each platform at each stage are judgement, kept in the page itself; everything this
script writes is counted. The Pages workflow runs it on every push to main.

Usage: python docs/coverage/build.py [--out site]
"""
import argparse
import json
import pathlib
import re
import subprocess
from collections import Counter

ROOT = pathlib.Path(__file__).resolve().parents[2]
HERE = pathlib.Path(__file__).resolve().parent
NODE = re.compile(r"^(\s*)- (\d+) ‹(\w+)› (.+?) \{c(\d)\}(?:.*?\[([^\]]+)\])?")
SLOT = "/*@MEASURED@*/null"


def text(path):
    return (ROOT / path).read_text(encoding="utf-8")


def lines(path):
    return len(text(path).splitlines())


def roadmap():
    """Every node of rhipper.done.md then rhipper.md; a milestone repeated in both keeps one entry."""
    nodes, milestones, order = {}, {}, []
    for name in ("rhipper.done.md", "rhipper.md"):
        milestone = None
        for line in text(name).splitlines():
            m = NODE.match(line)
            if not m:
                continue
            id_, kind, title, c, state = int(m.group(2)), m.group(3), m.group(4), int(m.group(5)), m.group(6) or ""
            if kind == "milestone":
                milestone = milestones.setdefault(id_, {"id": id_, "title": title, "tasks": [], "sections": []})
                if id_ not in order:
                    order.append(id_)
            elif kind in ("task", "subtask"):
                nodes[id_] = {"state": state, "c": c, "title": title}
                if milestone is not None and id_ not in milestone["tasks"]:
                    milestone["tasks"].append(id_)
            elif kind == "section":
                nodes[id_] = {"state": state, "c": c, "title": title, "section": True}
                if milestone is not None and id_ not in milestone["sections"]:
                    milestone["sections"].append(id_)
    return nodes, [milestones[i] for i in order]


def crates():
    rows = []
    for d in sorted([*(ROOT / "crates").iterdir(), ROOT / "apps" / "ras"]):
        files = list(d.rglob("*.rs"))
        source = [f.read_text(encoding="utf-8") for f in files]
        name = "ras (app)" if d.name == "ras" else d.name
        rows.append([name, sum(len(s.splitlines()) for s in source), sum(s.count("#[test]") for s in source)])
    return rows


def forms():
    def distinct(path, pattern):
        return len(set(re.findall(pattern, text(path))))

    return {
        "x86": distinct("crates/ra-asm-lift/src/x86.rs", r"\bM::([A-Z]\w+)"),
        "x87": distinct("crates/ra-asm-lift/src/x87.rs", r"\bM::([A-Z]\w+)"),
        # the AArch64 lift matches yaxpeax's opcodes by their bare names
        "a64": distinct("crates/ra-asm-lift/src/arm64.rs", r"\b([A-Z][A-Z0-9]{1,9})\b(?=\s*(?:\||=>|\(|\{))"),
        "a64_adapter": distinct("crates/ra-asm-arm64/src/decode.rs", r"\bOpcode::(\w+)"),
        "thumb": distinct("crates/ra-asm-lift/src/thumb.rs", r"\bOp::(\w+)"),
        "thumb_decoder_lines": lines("crates/ra-asm-arm64/src/thumb.rs"),
        # the MIPS machines are rows of one table, so their lift is counted by its lines
        "mips_lift_lines": lines("crates/ra-asm-lift/src/mips.rs"),
        "sh_decoder_lines": lines("crates/ra-asm-sh/src/decode.rs"),
    }


def runtimes():
    """The platform runtime of each console: its lines, its tests, and the console's committed
    fixtures with their golden outputs. `shared` is what the consoles have in common."""
    base = ROOT / "crates" / "ra-asm-platform" / "src"
    consoles = {
        "psx": ("psx", ["testdata/bin/*_psx.exe"], ["testdata/expected/*_psx.txt"]),
        "ps2": ("ps2", ["testdata/ps2/*.elf", "testdata/ps2/*.irx"], ["testdata/ps2/*.txt"]),
        "n64": ("n64.rs", ["testdata/n64/*.z64"], ["testdata/n64/*.txt"]),
        "psp": ("psp.rs", ["testdata/psp/*.prx"], ["testdata/psp/*.txt"]),
        "dreamcast": ("dreamcast.rs", ["testdata/dc/1ST_READ.BIN"], ["testdata/dc/*.txt"]),
    }

    def measure(files):
        source = [f.read_text(encoding="utf-8") for f in files]
        return {"lines": sum(len(s.splitlines()) for s in source), "tests": sum(s.count("#[test]") for s in source)}

    def found(patterns):
        return sum(len(list(ROOT.glob(p))) for p in patterns)

    out, own = {}, set()
    for name, (where, fixtures, golden) in consoles.items():
        path = base / where
        files = sorted(path.rglob("*.rs")) if path.is_dir() else [path]
        own.update(files)
        out[name] = {**measure(files), "fixtures": found(fixtures), "golden": found(golden)}
    out["shared"] = measure([f for f in sorted(base.glob("*.rs")) if f not in own])
    return out


def java():
    """The Java machine: the instructions the decoder names, and the releases with a committed
    class, with `javap`'s listing and with what the program prints."""
    names = re.search(r"pub const NAMES: \[&str; \d+\] = \[(.*?)\];", text("crates/ra-asm-jvm/src/decode.rs"), re.S)
    releases = {}
    for d in (ROOT / "testdata" / "java").iterdir():
        if d.name.isdigit() and (d / "Opcodes.class").exists():
            releases[int(d.name)] = {"listing": (d / "Opcodes.txt").exists(), "output": (d / "Opcodes.out").exists()}
    files = list((ROOT / "crates" / "ra-asm-jvm").rglob("*.rs"))
    source = [f.read_text(encoding="utf-8") for f in files]
    # the lift and the runtime: the test that runs every release's lifted fixture against what
    # `java` printed, and the classes of the library the runtime answers for
    run = ROOT / "crates" / "ra-asm-jvm" / "src" / "run.rs"
    library = re.search(r"const LIBRARY: .*? = &\[(.*?)\n\];", run.read_text(encoding="utf-8"), re.S) if run.exists() else None
    return {
        "runs": (ROOT / "crates" / "ra-asm-jvm" / "tests" / "run.rs").exists(),
        "library": len(re.findall(r'^\s*\("', library.group(1), re.M)) if library else 0,
        "opcodes": len(re.findall(r'"\w+"', names.group(1))) if names else 0,
        "releases": dict(sorted(releases.items())),
        "lines": sum(len(s.splitlines()) for s in source),
        "tests": sum(s.count("#[test]") for s in source),
    }


def boundary():
    files = {
        "Win32 protos": "crates/ra-asm-protos/src/win32.rs",
        "Win32 names": "crates/ra-asm-protos/src/win32_names.rs",
        "Win32 host (driver.c)": "runtime/driver.c",
        "interp host.rs": "crates/ra-asm-lift/src/run/host.rs",
        "rhipper.rs runtime": "runtime/rhipper.rs",
        "printf / os shim": ["crates/ra-asm-lift/src/run/printf.rs", "crates/ra-asm-lift/src/run/os.rs"],
    }
    out = []
    for k, v in files.items():
        paths = v if isinstance(v, list) else [v]
        out.append([k, sum(lines(p) for p in paths if (ROOT / p).exists())])
    return out


def machine(data):
    """(container, machine) of a fixture from its own header, or None."""
    if data[:2] == b"MZ" and len(data) > 0x40:
        pe = int.from_bytes(data[0x3C:0x40], "little")
        kind = int.from_bytes(data[pe + 4:pe + 6], "little")
        return "PE", {0x14C: "x86-32", 0x8664: "x86-64", 0xAA64: "AArch64"}.get(kind)
    if data[:4] == b"\x7fELF":
        # the machine is in the file's own byte order
        kind = int.from_bytes(data[18:20], "big" if data[5] == 2 else "little")
        return "ELF", {3: "x86-32", 62: "x86-64", 183: "AArch64", 40: "ARM32", 8: "MIPS"}.get(kind)
    if data[:8] == b"PS-X EXE":
        return "PS-X EXE", "MIPS"
    if data[:4] == b"\xcf\xfa\xed\xfe":
        kind = int.from_bytes(data[4:8], "little")
        return "Mach-O", {0x1000007: "x86-64", 0x100000C: "AArch64"}.get(kind)
    if data[:4] == b"XBEH":
        return "XBE", "x86-32"
    return None


def fixtures():
    cols = ["x86-32", "x86-64", "AArch64", "ARM32", "MIPS"]
    rows = {k: Counter() for k in ("PE", "ELF", "Mach-O", "XBE", "PS-X EXE", "UPX-packed", "with symbols")}
    bins = ROOT / "testdata" / "bin"
    for f in sorted(bins.iterdir()):
        if f.suffix in (".map", ".pdb", ".txt"):
            continue
        data = f.read_bytes()
        found = machine(data)
        if not found or not found[1]:
            continue
        container, arch = found
        rows[container][arch] += 1
        if b"UPX0" in data[:4096]:
            rows["UPX-packed"][arch] += 1
        if f.with_suffix(".map").exists() or f.with_suffix(".pdb").exists():
            rows["with symbols"][arch] += 1
    return {"cols": cols, "rows": [[k, [c[a] for a in cols]] for k, c in rows.items()]}


def git(*args):
    return subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True, check=True).stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="site")
    out = ROOT / ap.parse_args().out
    nodes, milestones = roadmap()
    tasks = {k: v for k, v in nodes.items() if not v.get("section")}
    complexity = Counter((v["c"], v["state"] == "accepted") for v in tasks.values())
    days = Counter(git("log", "--format=%ad", "--date=short").split())
    measured = {
        "head": git("rev-parse", "--short", "HEAD").strip(),
        "date": git("log", "-1", "--format=%ad", "--date=short").strip(),
        "commits": sorted(days.items())[-30:],
        "total_commits": sum(days.values()),
        "nodes": nodes,
        "milestones": milestones,
        "states": Counter(v["state"] for v in tasks.values()),
        "complexity": [[c, complexity[(c, True)], complexity[(c, False)]] for c in range(1, 6)],
        "crates": crates(),
        "forms": forms(),
        "boundary": boundary(),
        "runtimes": runtimes(),
        "java": java(),
        "fixtures": fixtures(),
        "golden": len(list((ROOT / "testdata" / "expected").glob("*.txt"))),
    }
    page = (HERE / "index.html").read_text(encoding="utf-8")
    if SLOT not in page:
        raise SystemExit(f"index.html has no {SLOT}")
    out.mkdir(parents=True, exist_ok=True)
    blob = json.dumps(measured, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    (out / "index.html").write_text(page.replace(SLOT, blob), encoding="utf-8")
    print(f"{out / 'index.html'}: {len(tasks)} cards, {sum(r[2] for r in measured['crates'])} tests, "
          f"{sum(r[1] for r in measured['crates'])} lines, {measured['golden']} golden outputs")


if __name__ == "__main__":
    main()
