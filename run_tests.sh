#!/bin/sh

set -eu

SCRIPT="
export PATH_ROOT  # The tests inject this

main() {
    # The tests source this file and call main() so it has to be a shell script
    # which executes our binary.  The tests also need to be run from
    # cloud-init's root, so specify a path relative to that.
    ../target/debug/rs-identify
}
"

setup_tree() {
    git clone https://github.com/canonical/cloud-init
    echo "$SCRIPT" > cloud-init/tools/ds-identify
}

main() {
    if ! grep rs-identify cloud-init/tools/ds-identify > /dev/null; then
        echo "Prepared cloud-init tree not detected; attempting to create"
        setup_tree
    fi
    cargo build
    (cd cloud-init; pytest tests/unittests/test_ds_identify.py)
}

main
