package iriscore.util

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings

/**
 * OEM battery-kill UX guidance (AC-7). Vendor ROMs aggressively kill background
 * services/work even with an FGS + WorkManager active; the shell surfaces a
 * one-tap "exempt this app" + vendor-specific instructions (user-initiated,
 * never a silent exemption). Pure data + intent builder — unit-tested on the
 * host Gradle `test` leg.
 */
object BatteryOptimizationGuidance {

    data class VendorGuidance(
        val vendor: String,
        val behavior: String,
        val steps: List<String>,
    )

    /** Request the user exempt the app from battery optimization (must be user-initiated). */
    fun ignoreBatteryOptimizationsIntent(context: Context): Intent =
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
            .setData(Uri.parse("package:${context.packageName}"))

    val matrix: List<VendorGuidance> = listOf(
        VendorGuidance(
            vendor = "samsung",
            behavior = "One UI Adaptive Battery + App Sleeping can freeze IRIS between windows.",
            steps = listOf(
                "Settings → Battery and device care → Battery → Background usage limits",
                "Set IRIS to 'Never sleeping apps'",
            ),
        ),
        VendorGuidance(
            vendor = "xiaomi",
            behavior = "MIUI/HyperOS aggressive app killers + 'Autostart' management.",
            steps = listOf(
                "Settings → Apps → Manage apps → IRIS → Battery saver → 'No restrictions'",
                "Turn on Autostart for IRIS",
            ),
        ),
        VendorGuidance(
            vendor = "oppo",
            behavior = "ColorOS battery saver + 'App freeze' in recent-tasks.",
            steps = listOf(
                "Settings → Battery → App battery management → IRIS → 'Allow background activity'",
            ),
        ),
        VendorGuidance(
            vendor = "realme",
            behavior = "realmeUI aggressive standby optimization.",
            steps = listOf(
                "Settings → Battery → App battery management → IRIS → allow background",
            ),
        ),
        VendorGuidance(
            vendor = "vivo",
            behavior = "Funtouch/OriginOS background app management.",
            steps = listOf(
                "Settings → Battery → Background app management → 'Allow background running'",
            ),
        ),
        VendorGuidance(
            vendor = "honor",
            behavior = "Honor MagicUI 'Smart battery' may pause background sync.",
            steps = listOf(
                "Settings → Battery → IRIS → ignore restrictions",
            ),
        ),
        VendorGuidance(
            vendor = "huawei",
            behavior = "EMUI/HarmonyOS 'App launch' manager restricts background work.",
            steps = listOf(
                "Settings → Apps → IRIS → App launch → 'Manage manually' → allow background",
            ),
        ),
        VendorGuidance(
            vendor = "google",
            behavior = "Pixel/Android Doze + App Standby buckets (network after standby 4+ h).",
            steps = listOf(
                "Settings → Apps → IRIS → Battery → 'Unrestricted'",
            ),
        ),
    )

    /** Known/aggressive OEM substring: lowercase manufacturer match. */
    fun isKnownVendor(manufacturer: String): Boolean =
        matrix.any { it.vendor in manufacturer.lowercase() }

    fun guidanceFor(manufacturer: String): VendorGuidance {
        val vendor = matrix.firstOrNull { it.vendor in manufacturer.lowercase() }
        return vendor ?: VendorGuidance(
            vendor = manufacturer.lowercase(),
            behavior = "Generic Android: Doze + App Standby buckets.",
            steps = listOf(
                "Settings → Apps → IRIS → Battery → 'Unrestricted'",
            ),
        )
    }
}