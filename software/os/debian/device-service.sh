#!/bin/sh
exec /usr/bin/systemctl "$1" "kvmpanel-${0##*/}.service"
