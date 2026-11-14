#!/bin/sh
# Builds DexNotas' highlight.js bundle (assets/highlightjs/dexhljs.js) and language table
# (src/syntax/hljs_langs.rs) from the versions pinned in package.json. Needs Node.js and npm.
set -e
cd "$(dirname "$0")"
npm install --no-audit --no-fund --silent
npx esbuild entry.js --bundle --minify --format=iife --legal-comments=none --log-level=warning \
  --outfile=../../assets/highlightjs/dexhljs.js
cp node_modules/highlight.js/LICENSE ../../assets/highlightjs/LICENSE
node gen_table.js
