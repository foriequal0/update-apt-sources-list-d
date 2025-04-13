# update-apt-sources-list-d

Enables and updates `/etc/apt/sources.list.d/*.sources` source entries.

1. Enables `Enabled: no` PPA sources.

2. Updates PPA sources that is pointing old dists.

   Such as from `xenial`, `zesty` to more recent one such as `eoan`, `focal`.

## Run

```
cargo build
sudo ./target/debug/update-apt-sources-list-d
```
