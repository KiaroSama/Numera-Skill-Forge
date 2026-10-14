#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
mkdir -p "$root/.ci-work/controlled-input"
work="$(mktemp -d "$root/.ci-work/controlled-input/run-XXXXXXXX")"
source="$(git rev-parse HEAD)"
producer="$root/target/release/forge-ci-input"
"$producer" project --repository "$root" --revision "$source" --output "$work/projection"
"$producer" verify --projection "$work/projection"
# Provisioning is outside controlled execution; no entire checkout is a build context.
if ! timeout --signal=TERM --kill-after=10s 900s docker build --platform=linux/amd64 \
  --file "$root/scripts/ci-input.Dockerfile" --build-arg "SOURCE_REVISION=$source" \
  --build-arg "MANIFEST_DIGEST=$(sha256sum "$work/projection/manifest.json" | cut -d' ' -f1)" \
  --iidfile "$work/image.id" "$work/projection" > "$work/preparation.log" 2>&1; then
  # The context contains public committed sources only, never host credentials.
  tail -n 60 "$work/preparation.log" >&2
  exit 1
fi
image="$(cat "$work/image.id")"
cleanup_image() { docker image rm "$image" > "$work/image-cleanup.log" 2>&1; }
trap cleanup_image EXIT
"$producer" run --image "$image" --source "$source" --mode probe --output "$work/probe"
"$producer" run --image "$image" --source "$source" --mode core --output "$work/core"
install -m 755 "$work/core/numera-forge" "$root/target/release/numera-forge"
"$producer" run --image "$image" --source "$source" --mode frontend --output "$work/frontend"
timeout --signal=TERM --kill-after=10s 180s python3 -I "$root/scripts/verify-ci-input.py" \
  --image "$image" --source "$source" --producer "$producer" --work "$work/negative"
if "$producer" run --image "$image" --source "$source" --mode timeout-probe --output "$work/timeout"; then
  echo 'Timeout regression unexpectedly succeeded' >&2
  exit 1
fi
python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1],encoding="utf-8")); assert d["outcome"]=="terminated" and d["cleanup_verified"] and d["exit"] is None' "$work/timeout/result.json"
# Never upload provisioning logs or arbitrary captured tool output.
