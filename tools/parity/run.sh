#!/usr/bin/env bash
# Parser parity check against the original TypeScript app.
# Usage: ORIG=/path/to/laffan/printfold tools/parity/run.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$here/../.."
: "${ORIG:?set ORIG to a checkout of github.com/laffan/printfold}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/orig"
cp -r "$ORIG/src/services/textFlow" "$ORIG/src/types" "$work/orig/"
echo "export const appState = { getProject: () => ({}) };" > "$work/orig/stub_state.ts"
sed -i.bak "s#from '../../types'#from '../types'#; s#from '../state'#from '../stub_state'#" "$work"/orig/textFlow/*.ts
cp "$here/run_orig.ts" "$work/"
(cd "$work" && npm init -y >/dev/null && npm install --silent marked@11.1.0 esbuild >/dev/null \
  && npx esbuild run_orig.ts --bundle --platform=node --outfile=run_orig.js --log-level=warning \
  && node run_orig.js "$here/corpus.json" > orig.json)
(cd "$root" && cargo run -q -p printfold-core --example parse_dump "$here/corpus.json") > "$work/rust.json"
python3 "$here/compare.py" "$here/corpus.json" "$work/orig.json" "$work/rust.json"
