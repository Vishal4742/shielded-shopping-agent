#!/bin/sh
# Rebuild web/diorama/diorama.js from main.js (not deployed: see web/.assetsignore).
# Needs three + esbuild somewhere outside web/, e.g.: mkdir -p tools/diorama-src && (cd tools/diorama-src && npm i three esbuild)
cd "$(dirname "$0")/../.." && NODE_PATH=tools/diorama-src/node_modules tools/diorama-src/node_modules/.bin/esbuild web/diorama-src/main.js \
  --bundle --format=esm --minify --target=es2020 --legal-comments=none \
  --banner:js="/* ASCII diorama hero: Mumbai street corner at dusk. Built on ascii-diorama, MIT licence, https://github.com/JDhruv14/ascii-diorama (licence text: diorama/LICENSE-ascii-diorama.txt). Bundles three.js (MIT, https://threejs.org). Source: web/diorama-src/main.js */" \
  --outfile=web/diorama/diorama.js
