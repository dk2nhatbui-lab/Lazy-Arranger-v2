# Test VST3 gain plug-in

A local-only CI fixture for the Lazy Arranger plug-in host. It is adapted from
the `vst3` crate's `gain` example, which is dual licensed MIT OR Apache-2.0.
The fixture adds a gain effect (one parameter, 32-sample latency) and a small
sine synth so host tests can exercise parameters, delay compensation and
instrument rendering.

Build it with:

```fish
./scripts/rust.sh build -p lazy-arranger-test-plugin
```

Host tests create a temporary `.vst3` bundle that points at the built library.
