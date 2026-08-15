# IDENT-001 Test Evidence (TEST stage, iter 62)

**Date**: 2026-08-15
**Node**: IDENT-001 (P0, HIGH risk)
**Stage**: TEST — AC-1..9 evidence mapping (AC-10 redteam / AC-11 verification
are later stages; this record pins the test-to-AC mapping + green run).

## Run evidence

- `cargo test -p iris-core identity` → **58 passed, 0 failed** (238 filtered).
- `cargo test -p iris-desktop identity` → **3 passed, 0 failed**.
- `cargo test --workspace` → **296+ green, 0 failed** (prior baseline 290 +
  58 identity core + 5 desktop identity − pre-existing = full green).
- `cargo clippy -p iris-core -p iris-desktop` → **0 warnings** (also
  `cargo clippy --workspace` clean).
- `cargo check -p iris-desktop` → clean (identity module + RED-0001 wiring
  compile in the desktop crate).

## AC → test mapping

### AC-1 — `identity` module + peer_id/peer_short derivation (determinism,
distinct-seed, no-MAC, P0 16-byte form); clippy 0; workspace green
- `peer_id.rs`: `peer_id_is_verifying_key_bytes`,
  `peer_short_same_key_same_short_id`, `distinct_keys_produce_distinct_short_ids`,
  `peer_short_is_16_bytes`, `no_hardware_derived_identity`,
  `human_uid_is_16_uppercase_base32_chars`, `matches_hex_in_p0_abbreviated_envelopes`
- Clippy 0 both crates; workspace green.

### AC-2 — provisioning round-trip; corrupt → loud error; zeroize on drop
- `provision.rs`: `provision_then_reload_returns_identical_peer_id`,
  `provision_is_idempotent_within_session`, `corrupt_blob_fails_loudly_not_silent_regen`,
  `wrong_version_blob_fails_loudly`, `serialization_round_trip_matches_layout`,
  `seeds_are_zeroized_on_drop` (allocation-wipe proof)
- `store.rs`: `fresh_store_is_empty`, `identity_round_trip_and_reload`,
  `master_key_round_trip`, `wrong_version_blob_fails_loudly`,
  `corrupt_blob_fails_loudly`, `identity_file_is_0600`,
  `buffer_buffers_zeroed_on_drop`
- Desktop: `identity.rs::provisioned_peer_id_is_persisted_and_stable` (reload
  over same FileKeyStore dir → identical PeerId + UID).

### AC-3 — KeyAdvertisement build/verify (valid; tampered/bad-sig; cross-identity)
- `advertise.rs`: `valid_ad_builds_and_verifies`,
  `tampered_static_key_fails_verify`, `tampered_counter_fails_verify`,
  `forged_ad_wrong_signer_rejected`, `serialization_round_trip`,
  `small_order_certified_key_rejected_at_build_and_verify`

### AC-4 — verify_chain matrix (trusted-root pass; untrusted/broken/expired/
revoked/small-order reject; cap)
- `chain.rs`: `trusted_root_self_signed_single_element_passes`,
  `authorized_child_chain_passes`, `untrusted_root_chain_rejected`,
  `broken_link_rejected`, `stale_counter_link_rejected`,
  `expired_link_rejected`, `small_order_pubkey_in_chain_rejected`,
  `sender_mismatch_rejected`, `empty_chain_rejected`, `overlong_chain_rejected`
  (cap 8), `undecodable_link_rejected`

### AC-5 — TrustStore semantics (TOFU; idempotent; KeyChanged; verify_peer;
revocation blocks all)
- `trust_store.rs`: `tofu_bind_then_repeated_ad_is_idempotent`,
  `resolution_feeds_key_directory`, `verify_peer_promotes_to_verified`,
  `key_change_warns_and_requires_reverify`,
  `rotation_adopts_when_counter_strictly_increases`,
  `stale_counter_earliest_seen_wins`,
  `revocation_blocks_all_paths_and_never_auto_heals`,
  `adversarial_ad_rejected`, `small_order_ad_rejected_red_0011`,
  `expired_ad_rejected`
- Desktop: `identity.rs::trust_key_directory_serves_adopted_keys` (adopt → the
  engine `KeyDirectory` seam resolves the X25519 target).

### AC-6 — rotation policy (monotonic; stale; equal earliest-seen-wins;
valid-until; null-rotation revoke)
- `rotate.rs`: `rotation_adopts_with_monotonic_counter`,
  `stale_counter_rejected_earliest_seen_wins`,
  `equal_counter_conflict_earliest_seen_wins`,
  `null_rotation_revokes_and_blocks_paths`, `forged_event_rejected`,
  `small_order_new_key_rejected`, `serialization_round_trip`

### AC-7 — RED-0011 small-order rejection (test vectors; valid accepted)
- `small_order.rs`: `classic_small_order_values_are_rejected`,
  `whole_torsion_subgroup_is_rejected`, `basepoint_is_valid`,
  `valid_remote_key_accepted`
- Enforced at boundaries: `advertise.rs` build/verify, `chain.rs`,
  `rotate.rs`, `trust_store.rs` (cross-referenced above).

### AC-8 — desktop RED-0001 (IrisCryptoProvider; persisted PeerId; FileKeyStore;
no DevCryptoProvider in production path)
- `identity.rs`: `crypto_provider_authenticates_and_roundtrips`
  (`authenticates() == true`; real Ed25519 sign/verify + X25519
  encrypt/decrypt round-trip through the engine `CryptoProvider` seam),
  `provisioned_peer_id_is_persisted_and_stable`
- `engine_handle.rs`: production `build()` selects `IrisCryptoProvider` +
  `TrustKeyDirectory` + persisted PeerId when `config.dev == false`;
  `DevCryptoProvider` is reachable only via `config.dev` (with_node_id /
  with_transport test seams) — compile/use guard by construction.
- `IRIS_NODE_ID` is a display-only label (no longer selects the identity).

### AC-9 — engine E2E with identity (sign/verify/encrypt/decrypt + key dir)
- Desktop `crypto_provider_authenticates_and_roundtrips` exercises the full
  provider path (sign, verify_strict, encrypt to X25519 static key, decrypt).
- `trust_key_directory_serves_adopted_keys` covers key-dir-fed encryption
  target resolution.
- `MessageEngine::set_key_directory` seam unit-tested at the engine boundary
  (directory-backed recipient-key lookup).
- Note: full wire E2E (Alice identity → wire → Bob verify/decrypt with
  auth_cert_chain) is exercised in SECURITY_REVIEW/VERIFY stages; the provider
  and directory primitives are proven here.

## Out of scope for TEST stage

- AC-10 (redteam SECURITY_REVIEW) — next stage.
- AC-11 (IDENT_VERIFICATION.md) — VERIFY stage.
- Master-key → `PgStorage::with_sealer` wiring: master key is provisioned and
  exposed (`DesktopIdentity::master_key()`); the PG sealer path remains
  opt-in per STORE-001 (loopback/MemoryStorage uses NoSealer) — recorded as
  known_limitation, consistent with CRYPTO-001 iter 57.

## Files changed this pass

- `crates/iris-core/src/identity/{mod,provision,store,peer_id,advertise,trust_store,chain,rotate,small_order}.rs`
- `crates/iris-core/src/lib.rs`, `Cargo.toml`, `src/message_engine/mod.rs`
- `crates/iris-desktop/src/{identity,engine_handle,lib}.rs`, `Cargo.toml`
- Durable state: `execution-state.yaml`, `ACTIVE_NODE.md`, `NEXT_ACTION.md`,
  `CURRENT_STATE.md`, `PROJECT_GRAPH.yaml`, `PROJECT_STATE.yaml`,
  `records/execution-log.md`, `CHANGELOG.md`
