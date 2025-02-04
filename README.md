# elfo

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/elfo.svg
[crates-url]: https://crates.io/crates/elfo
[docs-badge]: https://docs.rs/elfo/0.2.0-alpha.8/elfo
[docs-url]: https://docs.rs/elfo/0.2.0-alpha.8/elfo
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/loyd/elfo/blob/master/LICENSE
[actions-badge]: https://github.com/elfo-rs/elfo/actions/workflows/ci.yml/badge.svg
[actions-url]: https://github.com/elfo-rs/elfo/actions/workflows/ci.yml

Elfo is another actor system. Check [The Actoromicon](http://actoromicon.rs/).

**Note: although it's already actively used in production, it's still under development. Wait for v0.2 for public announcement.**

## Usage
To use `elfo`, add this to your `Cargo.toml`:
```toml
[dependencies]
elfo = { version = "0.2.0-alpha.8", features = ["full"] }

[dev-dependencies]
elfo = { version = "0.2.0-alpha.8", features = ["test-util"] }
```

Note: until [sharded-slab#80](https://github.com/hawkw/sharded-slab/pull/80) is merged, it should be added:
```toml
[patch.crates-io]
sharded-slab = { git = 'https://github.com/loyd/sharded-slab.git' }
```

## Examples
[Examples](examples).
