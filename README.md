# rs-identify

This is a reimplementation of
[cloud-init](https://github.com/canonical/cloud-init)'s
[ds-identify](https://github.com/canonical/cloud-init/blob/master/tools/ds-identify)
in Rust, intended only as a learning resource.

## License

As this project is not intended for any use but educational, it is
licensed under [The Cooperative Non-Violent Public
License](https://thufie.lain.haus/NPL.html).

## Tests

There is a `run-tests.sh` script provided in the repository, which
will:

* clone cloud-init
* replace the in-tree `ds-identify` with a pointer to
  `target/debug/rs-identify`
* `cargo build`
* run the ds-identify tests

(It will only perform the first two steps if necessary; if it fails,
wipe out the cloud-init tree before trying again.)
