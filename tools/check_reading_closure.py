"""Check Rust reads against known values in four local controlled AEP fixtures.

Inputs stay in place. Their authoring scripts define independent expected times,
vertices, zero tangent vectors, gradient stop changes and text/style boundaries.
No reference reader or After Effects installation is used by this check.
"""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import time

def nodes(entries):
    for node in entries:
        yield node
        yield from nodes(node["children"])

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture-root",required=True,type=Path)
    parser.add_argument("--artifact-dir",type=Path)
    parser.add_argument("--toolchain",default="stable")
    parser.add_argument("--offline",action="store_true")
    args=parser.parse_args();repo=Path(__file__).resolve().parents[1]
    artifact=(args.artifact_dir or repo/".task"/f"reading-closure-{time.time_ns()}").resolve()
    if repo/".task" not in artifact.parents:raise ValueError("artifacts must stay inside this repository's .task")
    artifact.mkdir(parents=True,exist_ok=True)
    env=os.environ.copy();env.setdefault("CARGO_TARGET_DIR",str(artifact/"target"));env["CARGO_TERM_COLOR"]="never"
    flags=subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0
    checks=[];commands=[]
    def run(command):
        result=subprocess.run(command,cwd=repo,env=env,text=True,encoding="utf-8",capture_output=True,creationflags=flags,timeout=300)
        commands.append({"command":[str(c) for c in command],"exit_code":result.returncode})
        (artifact/f"command-{len(commands)}.log").write_text(result.stdout+result.stderr,encoding="utf-8")
        if result.returncode:raise RuntimeError(result.stderr)
        return result.stdout
    def check(label,actual,expected):
        passed=actual==expected
        checks.append({"check":label,"passed":passed,"actual":actual,"expected":expected})
        if not passed:raise AssertionError(label)
    def equivalent(actual,expected):
        if isinstance(expected,list):return isinstance(actual,list) and len(actual)==len(expected) and all(equivalent(a,b) for a,b in zip(actual,expected))
        return isinstance(actual,(float,int)) and math.isclose(actual,expected,abs_tol=1e-4,rel_tol=1e-6)
    def close(label,actual,expected):
        passed=equivalent(actual,expected)
        checks.append({"check":label,"passed":passed,"actual":actual,"expected":expected})
        if not passed:raise AssertionError(label)
    def model(name,derived=False):
        binary=Path(env["CARGO_TARGET_DIR"])/"debug/examples"/("inspect_derived" if derived else "inspect")
        if os.name=="nt":binary=binary.with_suffix(".exe")
        return json.loads(run([str(binary),str((args.fixture_root/name).resolve())]))
    report={"toolchain":args.toolchain,"checks":checks,"commands":commands,"basis":"known local authoring inputs, not reference-parser equality"}
    try:
        build=["cargo",f"+{args.toolchain}","build","--locked","--examples","--features","serde"]+(["--offline"] if args.offline else [])
        run(build)
        expected=[[[0,0],[40,0],[40,40],[0,40]],[[10,10],[150,10],[150,150],[10,150]],[[0,0],[200,0],[100,200]]]
        for name in ["re_path_anim_ae2020.aep","re_path_anim_ae2025.aep"]:
            project=model(name);derived=model(name,True)
            streams=[n["property"] for c in project["compositions"] for l in c["layers"] for n in nodes(l["properties"]) if n["match_name"]=="ADBE Vector Shape" and n["property"]]
            check(name+" animated path count",len(streams),1)
            keys=streams[0]["keyframes"];close(name+" key times",[k["time_seconds"] for k in keys],[0,1,2])
            geometries=[n["geometry"] for c in derived["compositions"] for l in c["layers"] for n in l["nodes"] if n["geometry"]]
            check(name+" derived path count",len(geometries),1);rows=geometries[0]
            check(name+" keyframe association",[r["time_ticks"] for r in rows],[k["time_ticks"] for k in keys])
            check(name+" closed flags",[r["closed"] for r in rows],[True,True,True])
            for field in ["anchor","in_tangent","out_tangent"]:
                actual=[[[p["value"] for p in v[field]] for v in r["layer_vertices"]] for r in rows]
                close(name+" layer-space "+field,actual,expected)
            check(name+" orphan snapshots",len(streams[0]["unmatched_path_snapshots"]),0)
        name="v2_2_gradient_anim_src.aep";project=model(name)
        animated=[n["property"] for c in project["compositions"] for l in c["layers"] for n in nodes(l["properties"]) if n["property"] and len(n["property"]["gradient_snapshots"])==2]
        check(name+" animated gradient count",len(animated),1);snapshots=animated[0]["gradient_snapshots"]
        close(name+" key times",[s["timing"]["time_seconds"] for s in snapshots],[0,1])
        close(name+" left stop changes red to green",[[n["value"] for n in s["gradient"]["color_stops"][0]["color"]] for s in snapshots],[[1,0,0],[0,1,0]])
        check(name+" original ticks",[s["timing"]["time_ticks"] for s in snapshots],[0,30720])
        name="re_text_multirun.aep";project=model(name)
        layers=[l for c in project["compositions"] for l in c["layers"] if l["name"]=="multirun_src"]
        check(name+" controlled layer",len(layers),1);text=layers[0]["text_source"]
        check(name+" stored text",text["text"],"Hello\r")
        check(name+" UTF-16 style ranges",[r["range"] for r in text["style_runs"]],[{"utf16_start":0,"utf16_length":2},{"utf16_start":2,"utf16_length":4}])
    finally:
        (artifact/"reading-closure-check.json").write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding="utf-8")
    print(json.dumps({"checks_passed":len(checks),"toolchain":args.toolchain,"artifact_dir":str(artifact)},ensure_ascii=False))

if __name__=="__main__":main()
