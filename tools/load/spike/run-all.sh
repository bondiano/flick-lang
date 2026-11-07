#!/usr/bin/env bash
cd /home/dev/worktrees/flick-lang/flick-lang-spike
L=tools/load/spike/load.sh
export CLIENT_JS=
$L ssr tools/load/ssr/dist/js/flick-server.js tools/load/ssr/dist/rt/flick-server.js '/render/{id}' 'request {id}' 5
$L json2 tools/load/json/dist/js/flick-server.js tools/load/json/dist/rt/flick-server.js '/item/{id}' '"item {id}"' 5
$L json-nd tools/load/json/dist/js/flick-server.js tools/load/json/dist/nd/flick-server.js '/item/{id}' '"item {id}"' 5
$L ssr-nd tools/load/ssr/dist/js/flick-server.js tools/load/ssr/dist/nd/flick-server.js '/render/{id}' 'request {id}' 5
echo all-load-done
