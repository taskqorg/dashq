#!/bin/sh
# D2: bridge pre-0.19 CoreQ updaters to the explicit-root selftest handoff.
set -eu
if [ "$#" -ne 1 ]; then
    echo 'usage: update-handoff.sh INSTALL' >&2
    exit 2
fi
install=$(CDPATH= cd -- "$1" && pwd -P)
# Invoke the official launcher, preserving its artifact and all CoreQ gates.
# An explicit root is inherited by the staged candidate's offline selftest.
DASHQ_INSTALL="$install" exec "$install/dashq" update
