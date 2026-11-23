# Test VST3 gain plug-in

A local-only CI fixture for the Lazy Arranger plug-in host. It is adapted from
the `vst3` crate's `gain` example, which is dual licensed MIT OR Apache-2.0.
The fixture adds a unique class id, one gain parameter and a reported latency of
32 samples so host tests can exercise parameters and delay compensation.

Build it with:

```fish
./scripts/rust.sh build -p lazy-arranger-test-plugin
```

Host tests create a temporary `.vst3` bundle that points at the built library.
