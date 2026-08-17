# No-secrets-in-logs grep guard (SEC-001 AC-16)
#
# Scope: PRODUCTION code (crates/*/src/) is enforced hard (exit 1 on violation).
# Test harness (crates/*/tests/) prints env-var NAMES like "IRIS_PG_PASSWORD unset"
# to say a schema test is skipped — the secret VALUE is never logged — so
# test-only hits are reported as warnings and do not fail the guard.
#
# Failure patterns: log/emission call sites (log::*, tracing::*, println!,
# eprintln!, .emit(, event()...) in src/ that transmit secret-bearing values:
# master_key, private_seed, ed25519_seed, static_x25519_secret, passphrase,
# password, sos_payload, or a non-None auth_cert_chain.
#
# Usage:
#   pwsh -File engineering/tools/secrets-log-guard.ps1            # default scan
#   pwsh -File engineering/tools/secrets-log-guard.ps1 -Repo <dir>  # custom root
#
# Exit 0 = clean, 1 = production violations found (for CI wiring). No CI pipeline
# exists yet in this repo; wire this script into CI once infrastructure is added.

param(
    [string]$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\.."))
)

$ErrorActionPreference = 'Stop'
$violations = @()
$testWarnings = @()

$logSites = @(
    'log::error!', 'log::warn!', 'log::info!', 'log::debug!', 'log::trace!',
    'println!', 'eprintln!', 'tracing::error!', 'tracing::warn!',
    '.emit(', 'event('
)

$secretSyms = @(
    'master_key', 'private_seed', 'ed25519_seed', 'static_x25519_secret',
    'passphrase', 'password', 'sos_payload'
)

# auth_cert_chain is checked separately (fields appear as `auth_cert_chain:` in
# struct literals AND as values passed to audit); skip identically-named None
# literals but flag any non-None value referenced within a log/audit call.
$files = Get-ChildItem -Path (Join-Path $Repo "crates") -Recurse -Include *.rs -File

foreach ($f in $files) {
    $lines = Get-Content -LiteralPath $f.FullName
    $text = ($lines -join "`n")
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        $siteIdx = -1
        foreach ($site in $logSites) {
            $k = $line.IndexOf($site, [StringComparison]::OrdinalIgnoreCase)
            if ($k -ge 0) { $siteIdx = $k; break }
        }
        if ($siteIdx -lt 0) { continue }
        # join up to 4 following lines (macro spans lines) for the call body
        $body = ($lines[$i..([Math]::Min($i + 4, $lines.Count - 1))] -join "`n")
        # auth_cert_chain: non-None value in a log/audit call
        if ($body -match 'auth_cert_chain\s*:\s*(?!None)') {
            $note = "$(Resolve-Path -Relative $f.FullName):$($i+1) [$site] passes auth_cert_chain value"
            if ($f.FullName -match '\\src\\') { $violations += $note } else { $testWarnings += $note }
        }
        foreach ($sec in $secretSyms) {
            if ($body.IndexOf($sec, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $note = "$(Resolve-Path -Relative $f.FullName):$($i+1) [$site] references [$sec]"
                if ($f.FullName -match '\\src\\') { $violations += $note } else { $testWarnings += $note }
            }
        }
    }
}

if ($testWarnings.Count -gt 0) {
    "TEST-HARNESS NOTES (env-var names / skip guards; not secret values):"
    $testWarnings | Sort-Object -Unique | ForEach-Object { "  $_" }
}

if ($violations.Count -gt 0) {
    "SECRET-LOG-GUARD VIOLATIONS (production src/):"
    $violations | Sort-Object -Unique | ForEach-Object { "  $_" }
    exit 1
}

"No-secrets-in-logs guard CLEAN for production src/ ($($files.Count) .rs files scanned)."
exit 0