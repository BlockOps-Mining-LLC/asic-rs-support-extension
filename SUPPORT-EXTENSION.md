# ASIC-RS Support Extension

A public extension of [256foundation/asic-rs](https://github.com/256foundation/asic-rs)
for additional ASIC models and firmware telemetry. Anyone can use, fork, or
contribute under the retained Apache-2.0 license. Upstream history is preserved;
the complete extension is maintained on `support-extension-runtime`. The
`support-extension` branch contains model additions proposed upstream separately.

## First coverage additions

- Separate stock Bitmain identities for **S21 XP Hyd**, **S21j XP Hyd**, and
  **S23 Hyd**. Case, make prefixes, whitespace, and exact `Hyd` / `Hyd.` /
  `Hydro` spellings are normalized without accepting arbitrary future variants.
- Modern `chain[]` and legacy `chain_rateN` telemetry parsing, with explicit
  hashrate units and separate board, chip, and coolant measurements.
- Optional per-board `inlet_fluid_temperature` and `outlet_fluid_temperature`
  fields, in Celsius. Older Python snapshots can omit these fields.
- Unknown hardware chip counts remain unknown instead of becoming zero.
- Python builds include the IANA timezone database dependency on Windows.

New parser fixtures are synthetic API examples. Passing fixture tests establishes
parser behavior, not live compatibility with every firmware release. Private
diagnostics, credentials, pool settings, and worker identities are not included.

Hardware counts were verified through read-only `stats`/`version` RPC telemetry
from 14 physical stock miners across eight firmware cohorts. Each model has three
boards: **S21 XP Hyd has 160 ASICs per board**, **S21j XP Hyd has 42**, and
**S23 Hyd has 84**. RPC `chain_acn` counts matched the working ASIC markers;
`stats.cgi` independently confirmed the same `asic_num` values and three board
indexes on a representative of each model. Formatting padding was excluded.
See the [hardware evidence in the model PR](https://github.com/256foundation/asic-rs/pull/409).
Existing S21 Pro+, S21+ Hyd, and S21e XP Hyd identities were already upstream.

## Verification boundary

These additions cover model identification and telemetry parsing. They do not
establish that a miner control command works on each model and firmware release.
Inherited firmware controls are unchanged and require device-specific checks.
Applications should validate control behavior separately before enabling
automation for newly recognized hardware.

Hiveon and IceRiver coverage require distinct protocol implementations and
representative response fixtures; they are not included in these additions.

## Using and contributing

The fork works independently as a Rust library with Python and Go bindings.
The Python distribution is `pyasic-rs-support-extension==0.8.5.post2`; its public
import remains `pyasic_rs`. Install the wheel for your platform from the
[fork binary release](https://github.com/BlockOps-Mining-LLC/asic-rs-support-extension/releases/tag/pyasic-rs-support-extension-v0.8.5.post2)
with pip. Do not install upstream `pyasic-rs` into the same environment: both
distributions own the same import namespace.

The release provides standard CPython 3.11–3.14 ABI3 wheels for Linux x86_64,
aarch64, ARMv7, and Windows x64, with SHA-256 checksums and a source revision
manifest. Linux x86_64/aarch64 require glibc 2.28; ARMv7 requires glibc 2.17.
Free-threaded Python is not supported. The fork workflow builds and tests these
assets before publishing them to GitHub; it does not publish to PyPI or crates.io.
Registry packages linked in the upstream README refer to upstream releases.

For source development, check out `support-extension-runtime` and use the
commands below. A source build requires Rust and the platform toolchain; Python
bindings additionally require Python and Maturin. Add `--features python,abi3`
when building a stable-ABI wheel.

To add a model or firmware, provide sanitized response fixtures with documented
units and the exact firmware version. Keep unknown measurements as unknown,
separate coolant from board/chip sensors, and add regression tests for stopped
miners and missing fields. Remove credentials, workers, serial numbers, and
private network details before submitting fixtures. Model recognition alone
must not be reported as verified control support.

## Manufacturer references

- [S21 XP Hyd specifications](https://support.bitmain.com/hc/en-us/articles/34523540504857-S21-XP-Hyd-Specification)
- [S21 XP Hyd user guide](https://file12.bitmain.com/shop-product-s3/firmware/6ced87e3-bfee-4525-ab66-4ac67d342398/2025/02/27/17/S21%20XP%20Hyd.%20User%20Guide-V4.0.17.pdf)
- [S23 Hyd manual](https://file12.bitmain.com/shop-product-s3/firmware/0dd419ca-c3e8-4cdf-8dba-f39db1b61354/2025/06/03/09/S23%20Hyd.%20Product%20Manual_v1.0.5.pdf)
- [BITMAIN catalogue](https://m.bitmain.com/)

## Development checks

```sh
cargo fmt --all -- --check
cargo test --all --locked --exclude asic-rs-pydantic --exclude asic-rs-pydantic-macros --exclude pyasic-rs --exclude asic-rs-ffi
cd python/pyasic-rs
maturin develop --features python --extras test
python -m pytest tests
```

For local development, build the fork in an isolated Python environment. Do not
publish its wheel under the upstream release version or upload it to upstream's
package namespaces.

## Validation

Before the metadata correction, the complete Rust workspace suite passed on
Windows x64 (307 tests), as did
the Python suite against a locally built native extension (132 tests). Go's
telemetry types and hardware helpers passed 17 isolated tests; full Go/FFI
integration is covered by the Linux CI workflow. Formatting and generated
supported-device documentation checks passed. These are offline checks and
do not establish live hardware or firmware-control acceptance.
