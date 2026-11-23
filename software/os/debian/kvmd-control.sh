#!/bin/sh

case "${1:-}" in
    start|stop|restart) exec /usr/bin/systemctl --no-block "$1" kvmd.service ;;
    *) echo "Usage: $0 {start|stop|restart}" >&2; exit 2 ;;
esac
