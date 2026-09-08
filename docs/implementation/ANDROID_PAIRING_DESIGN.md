# Android Trusted-Peer Pairing UX

**Node:** ANDROID-PAIR-001  
**Status:** DESIGN COMPLETE  
**Research:** RES-0033

## Selected flow

`Contacts` opens from the console status line. A user may show their signed QR,
scan another phone, or paste the same signed code received remotely. The app
validates and adopts the advertisement as Unverified, displays the SAS and peer
fingerprint, and requires an alias plus an explicit "codes match" confirmation.
Only a successful core `verifyPeer` call persists the alias as verified.

Contacts are local display metadata. Selecting one sets the existing raw PeerId
recipient; `/to <alias>` remains a second path over the same resolver. Rename
does not affect identity. Forget removes both alias and trusted advertisement.

## Failure and security behavior

- Malformed, oversized, wrong-version, unsigned or rejected payloads stop at
  preview; no alias is saved.
- Key-change, revoked and rejected outcomes are blocking states, never silently
  overwritten.
- Duplicate aliases are refused case-insensitively; blank/control-character and
  overlong aliases are refused.
- Scanner processes QR only, drops frames while busy, closes each image, stops
  after the first valid IRIS payload, and releases camera/scanner resources.
- Bare PeerId + X25519 manual trust is deliberately unsupported because it
  removes the Ed25519 binding. Remote users copy the complete signed code.

## Acceptance criteria

1. Own signed advertisement renders as a scannable QR and can be copied.
2. Bundled offline scanner reads only QR and feeds the same parser as paste.
3. Manual paste accepts versioned IRIS codes and legacy raw advertisement hex.
4. Pairing cannot save an alias until core adoption and SAS confirmation pass.
5. Verified aliases persist across process restart and resolve in `/to name`.
6. Contact picker selects the correct raw PeerId; rename and forget work.
7. Duplicate/blank/overlong aliases and malformed/oversized payloads fail safely.
8. Key-change/revoked/rejected outcomes remain blocked.
9. Unit tests cover codec, alias policy, command resolution and QR generation;
   Android JVM suite, Rust trust/SAS tests and debug APK build pass.
10. New APK is installed on both connected original phones; messaging remains a
    human-observed hardware gate and is not claimed automatically.
