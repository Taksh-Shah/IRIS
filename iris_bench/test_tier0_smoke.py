"""Tier-0 gate smoke — the trivial "P1 sends, P2 receives" test.

Loop §2: the gate to Tier 1 is this passing **10/10** with the evidence pipeline
emitting logcat + dumpsys + meshSnapshot (+ btsnoop where available).

Payload is a 2-byte string on purpose: one ATT write, no fragmentation — this
isolates the harness + the single-hop delivery path from HV-7 (fragmentation).
Larger payloads and the reply direction are Tier-1 tests.
"""

import json

from mobly import asserts
from mobly import test_runner

from iris_bench.base import IrisBenchBase

_ITERATIONS = 10
_PAYLOAD = "hi"
_DELIVER_TIMEOUT_MS = 90_000


class Tier0Smoke(IrisBenchBase):

    def setup_test(self):
        self.start_mesh_both()

    def teardown_test(self):
        self.stop_mesh_both()

    def test_p1_sends_p2_receives_10x(self):
        results = []
        for i in range(_ITERATIONS):
            r = self.send_and_await(
                self.p1, self.p2, f"{_PAYLOAD}{i}",
                priority=4, timeout_ms=_DELIVER_TIMEOUT_MS,
            )
            results.append(r)
            self.p1.log.info("iter %d/%d: %s", i + 1, _ITERATIONS, r)

        self.capture_evidence("tier0_smoke")
        delivered = [r for r in results if r.get("delivered")]
        latencies = sorted(r["latencyMs"] for r in delivered if r.get("latencyMs", -1) >= 0)
        summary = {
            "delivered": len(delivered),
            "of": _ITERATIONS,
            "median_latency_ms": latencies[len(latencies) // 2] if latencies else None,
            "results": results,
        }
        self.p1.log.info("SMOKE SUMMARY: %s", json.dumps(summary))

        asserts.assert_equal(
            len(delivered), _ITERATIONS,
            f"P1->P2 single-hop must be 10/10; got {len(delivered)}/{_ITERATIONS}. "
            f"results={results}",
        )


if __name__ == "__main__":
    test_runner.main()
