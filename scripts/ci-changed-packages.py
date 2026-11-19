#!/usr/bin/env python3
"""The workspace packages a CI run tests: the crates its push changed.

CI compiles every crate and test target on every push (`cargo check`/`clippy --all-targets`);
this picks the crates whose tests then run. A path under `crates/<dir>/` names that crate's
package. Nothing else does: a change to the root manifest, `Cargo.lock`, `third_party/`, scripts
or docs runs no tests, and a changed crate's dependents are not tested. A `workflow_dispatch`
with `all` tests the whole workspace.

Environment:
  BASE  the commit to diff against: the push's `before`, or a pull request's base. Empty or all
        zeros (a new branch, a dispatch): `origin/main`.
  ALL   `true`: every package.

Writes to `$GITHUB_OUTPUT` (stdout without it):
  args      Cargo package arguments (`-p a -p b`, or `--workspace`), empty when nothing changed
  packages  the names, space separated and space padded, for `contains(..., ' name ')`
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout


def package_name(crate_dir: Path) -> str | None:
    manifest = crate_dir / "Cargo.toml"
    if not manifest.is_file():
        return None
    in_package = False
    for line in manifest.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[package]"
        elif in_package:
            match = re.match(r'name\s*=\s*"([^"]+)"', stripped)
            if match:
                return match.group(1)
    return None


def all_packages() -> list[str]:
    names = (package_name(d) for d in sorted((ROOT / "crates").iterdir()) if d.is_dir())
    return [name for name in names if name]


def changed_paths(base: str) -> list[str]:
    if not base or set(base) == {"0"}:
        git("fetch", "--no-tags", "--depth=1", "origin", "+refs/heads/main:refs/remotes/origin/main")
        base = "origin/main"
    else:
        try:
            git("cat-file", "-e", f"{base}^{{commit}}")
        except subprocess.CalledProcessError:
            git("fetch", "--no-tags", "--depth=1", "origin", base)
    return git("diff", "--name-only", base, "HEAD").splitlines()


def changed_packages(paths: list[str]) -> list[str]:
    names = set()
    for path in paths:
        parts = path.split("/")
        if len(parts) > 2 and parts[0] == "crates":
            name = package_name(ROOT / "crates" / parts[1])
            if name:
                names.add(name)
    return sorted(names)


def main() -> int:
    if os.environ.get("ALL", "").lower() == "true":
        names = all_packages()
        args = "--workspace"
    else:
        names = changed_packages(changed_paths(os.environ.get("BASE", "").strip()))
        args = " ".join(f"-p {name}" for name in names)
    print("tested packages:", ", ".join(names) if names else "none")
    lines = [f"args={args}", f"packages= {' '.join(names)} "]
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
    else:
        print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
