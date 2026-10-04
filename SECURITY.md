# Security

## Reporting

Please report vulnerabilities privately to the maintainers via GitHub
security advisories on `Exonical/tak-rs` rather than public issues.

## Threat model (phase 1)

TAK-RS parses data from the network that may be malicious or malformed:
other clients on a shared TAK Server, multicast peers, or imported files.
Phase-1 code treats all input as untrusted.

| Threat                              | Mitigation                                                        |
|-------------------------------------|-------------------------------------------------------------------|
| XML entity expansion (billion laughs) | DTDs rejected; only predefined/numeric character refs expanded   |
| XXE / external entities             | DTDs rejected; no network or file access from the parser          |
| Oversized documents / frames        | `ParseLimits::max_bytes`, `TransportConfig::max_frame_bytes`; declared protobuf length checked before buffering |
| Deep nesting / node floods          | `ParseLimits::max_depth`, `max_nodes`, `max_attributes`           |
| Panics as DoS                       | `#![forbid(unsafe_code)]`, `unwrap`/`expect`/`panic` lints, fuzzing, proptest |
| Slow-loris / stalled peers          | Connect, write and idle read timeouts                            |
| Injection into re-emitted XML       | Names and characters validated on write; attributes/text escaped  |
| Invalid coordinates / values        | Validated constructors in `tak-core`; no silent clamping          |

## Cryptography and secrets

* TLS via `rustls` only; OpenSSL and native-tls are banned in `deny.toml`.
* mTLS client identity from PEM or PKCS#12; private keys are held in
  `Zeroize`-wrapped buffers and never `Debug`-printed, logged or serialised.
* No secrets in `tracing` output; connection logs record host, port, cert
  subject/fingerprint — never key material or PKCS#12 passwords.
* Certificate validation is on by default (`Verification::Full`). Two
  explicit relaxations exist and are the caller's choice, never a fallback:
  `TrustedChainAnyName` (`--no-verify-hostname`: chain still validated,
  hostname ignored — common for TAK servers reached by IP) and
  `DangerousNoVerify` (`--insecure`: logged at `warn`, debugging only).
* `tak cert inspect` and `CertInfo` expose public certificate facts only; a
  PKCS#12 file's key is loaded into zeroised memory and then dropped.
* Inbound frames that fail to decode are counted (`SessionStats.decode_errors`)
  and skipped; they never abort the session or panic.

## Supply chain

* `cargo-deny` runs in CI: advisories, licence allow-list, banned crates
  (FFmpeg bindings, OpenSSL), unknown registries/git sources.
* `Cargo.lock` is committed; CI builds `--locked`.
* Dependencies are chosen for maturity (tokio, quick-xml, rustls, prost).

## Plugins

Future plugins use a WASM component sandbox with capability-based imports; a
plugin cannot touch the network, filesystem or certificates unless granted.
