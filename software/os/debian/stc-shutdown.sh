#!/bin/sh

# systemd has already stopped services, swap and writable disk mounts.
if [ "$1" = poweroff ] && [ -c /dev/ttyS2 ]; then
    exec /usr/libexec/kvmpanel/stc-power late-poweroff
fi
