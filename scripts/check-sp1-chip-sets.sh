#!/usr/bin/env bash
# Checks RECURSION_CHIPS in crates/host/src/pipeline/diagnostics.rs against the chips SP1 actually
# puts on each machine. The shard classifier reads that set as one machine and its complement as
# the other, so it breaks if SP1 adds a recursion chip, or gives a RISC-V chip a recursion name.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
diagnostics="$root/crates/host/src/pipeline/diagnostics.rs"

src_of() {
  cargo metadata --format-version 1 --manifest-path "$root/Cargo.toml" |
    python3 -c "
import json,sys,pathlib
want=sys.argv[1]
for p in json.load(sys.stdin)['packages']:
    if p['name']==want:
        print(pathlib.Path(p['manifest_path']).parent); break
else:
    sys.exit(f'{want} is not in the dependency graph')
" "$1"
}

names_in() { grep -rhoE '"[A-Za-z0-9_]+"' "$@" | tr -d '"' | sort -u; }

recursion_src="$(src_of sp1-recursion-machine)"
core_src="$(src_of sp1-core-machine)"
executor_src="$(src_of sp1-core-executor)"

# Every chip of the recursion machines lives under src/chips, one `fn name()` each.
sp1_recursion="$(
  grep -rhA6 'fn name(&self)' "$recursion_src/src/chips" |
    grep -oE '^\s+("[A-Za-z0-9_]+"|[0-9]+ => "[A-Za-z0-9_]+")' |
    grep -oE '"[A-Za-z0-9_]+"' | tr -d '"' | sort -u
)"

# The RISC-V side is open-ended: take both the chips' own `fn name()` strings and RiscvAirId.
sp1_core="$(
  {
    grep -rhA8 'fn name(&self)' "$core_src/src" | grep -oE '^\s+"[A-Za-z0-9_]+"' | tr -d ' "'
    awk '/^pub enum RiscvAirId/{f=1;next} f&&/^\}/{exit} f' "$executor_src/src/air.rs" |
      grep -oE '^\s{4}[A-Z][A-Za-z0-9_]*' | tr -d ' '
  } | sort -u
)"

ours="$(
  awk '/pub\(crate\) const RECURSION_CHIPS/{f=1;next} f&&/^\];/{exit} f' "$diagnostics" |
    grep -oE '"[A-Za-z0-9_]+"' | tr -d '"' | sort -u
)"

fail=0
report() {
  local label="$1" list="$2"
  [ -z "$list" ] && return 0
  printf '\n%s\n' "$label"
  printf '  %s\n' $list
  fail=1
}

report "SP1 recursion chips missing from RECURSION_CHIPS (recursion shards would read as core):" \
  "$(comm -23 <(echo "$sp1_recursion") <(echo "$ours"))"
report "In RECURSION_CHIPS but on no recursion machine (stale, or moved to the RISC-V machine):" \
  "$(comm -13 <(echo "$sp1_recursion") <(echo "$ours"))"
report "On BOTH of SP1's machines (the classifier cannot tell these shards apart):" \
  "$(comm -12 <(echo "$sp1_recursion") <(echo "$sp1_core"))"
report "RISC-V chips sharing a name with RECURSION_CHIPS (core shards would read as recursion):" \
  "$(comm -12 <(echo "$sp1_core") <(echo "$ours"))"

if [ "$fail" -ne 0 ]; then
  printf '\nFAIL: update RECURSION_CHIPS in %s\n' "${diagnostics#"$root/"}"
  exit 1
fi
printf 'ok: %s recursion chips match SP1, and none collide with the RISC-V machine\n' \
  "$(echo "$ours" | wc -l | tr -d ' ')"
