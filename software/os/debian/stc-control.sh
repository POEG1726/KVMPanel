#!/bin/sh
set -eu

helper=/usr/libexec/kvmpanel/stc-power
if [ "${1:-}" = update ]; then
    exec /usr/bin/systemd-inhibit --what=shutdown --mode=block \
        --who=KVMPanel --why="STC8 update holds the T113 power and reset lines" \
        "$helper" "$@"
fi
exec "$helper" "$@"
