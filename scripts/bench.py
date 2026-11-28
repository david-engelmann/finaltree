#!/usr/bin/env python3
"""Reproducible synthetic benchmark harness for finaltree."""
import argparse, os, pathlib, statistics, subprocess, tempfile, time

def median_ms(fn, repeats=3):
    fn()
    samples = []
    for _ in range(repeats):
        t0 = time.perf_counter()
        fn()
        samples.append((time.perf_counter() - t0) * 1000.0)
    return statistics.median(samples)

def main():
    ap = argparse.ArgumentParser(description="Run synthetic finaltree benchmarks")
    ap.add_argument("--bin", default="target/release/finaltree", help="Path to finaltree binary")
    args = ap.parse_args()
    bin_path = str(pathlib.Path(args.bin).resolve())
    git_env = {
        **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": "NUL" if os.name == "nt" else "/dev/null",
        "GIT_AUTHOR_NAME": "Synthetic",
        "GIT_AUTHOR_EMAIL": "synthetic@example.invalid",
        "GIT_COMMITTER_NAME": "Synthetic",
        "GIT_COMMITTER_EMAIL": "synthetic@example.invalid",
    }

    print("| Workload | fingerprint (ms) | verify (ms) |")
    print("| --- | ---: | ---: |")
    with tempfile.TemporaryDirectory() as tmp:
        tdir = pathlib.Path(tmp)
        for n_files in [20, 60, 150]:
            rdir = tdir / f"repo-{n_files}"
            log_p = tdir / f"stamped-{n_files}.log"
            rdir.mkdir(parents=True, exist_ok=True)
            subprocess.run(["git", "-C", str(rdir), "init", "-q"], env=git_env, check=True)
            for i in range(n_files):
                fp = rdir / f"dir_{i // 20:02d}" / f"file_{i:04d}.txt"
                fp.parent.mkdir(parents=True, exist_ok=True)
                fp.write_text(f"synthetic content {i}\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(rdir), "add", "."], env=git_env, check=True)
            subprocess.run(["git", "-C", str(rdir), "-c", "commit.gpgsign=false", "commit", "-q", "-m", "init"], env=git_env, check=True)
            subprocess.run([bin_path, "stamp", str(rdir), "--log", str(log_p), "--", "git", "--version"], env=git_env, check=True, capture_output=True)
            f_ms = median_ms(lambda: subprocess.run([bin_path, "fingerprint", str(rdir)], env=git_env, check=True, capture_output=True))
            v_ms = median_ms(lambda: subprocess.run([bin_path, "verify", str(log_p), str(rdir)], env=git_env, check=True, capture_output=True))
            print(f"| {n_files} tracked files | {f_ms:.2f} | {v_ms:.2f} |")

if __name__ == "__main__":
    main()
