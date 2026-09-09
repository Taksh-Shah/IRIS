package iriscore.designsystem

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/**
 * IRIS palette — deep black first, glass second, accent last.
 *
 * The interface is deliberately near-monochromatic: roughly 95% black and
 * charcoal, 4% white and grey, 1% accent. Hierarchy comes from typography,
 * spacing, depth and material — never from colour. The design test is that
 * removing every accent below should leave the interface intact; if it looks
 * flat without them, the hierarchy is wrong and the fix is spacing and type,
 * not more colour.
 */
@Immutable
object IrisColors {

    // -- environment -------------------------------------------------------

    /** True black. OLED panels drive these pixels dark, which is the point. */
    val BackgroundPrimary = Color(0xFF000000)
    val BackgroundSecondary = Color(0xFF050505)

    // -- surfaces ----------------------------------------------------------

    val SurfacePrimary = Color(0xFF0A0A0A)
    val SurfaceSecondary = Color(0xFF0F0F0F)

    /** Raised surface for the rare element that must read above the rest. */
    val SurfaceRaised = Color(0xFF141414)

    // -- text --------------------------------------------------------------

    // Contrast ratios below are against BackgroundPrimary (#000000) and
    // SurfacePrimary (#0A0A0A). WCAG AA needs 4.5:1 for body text; all four
    // tiers clear it on both. The earlier #666666 (3.66:1) and #3D3D3D
    // (1.93:1) did not — timestamps, placeholders and system lines were set in
    // them, which is exactly the small text that most needs the contrast.

    /** Not pure white: #FFF on true black glares and vibrates on OLED. 19.3:1 */
    val TextPrimary = Color(0xFFF5F5F5)

    /** 8.1:1 */
    val TextSecondary = Color(0xFFA1A1A1)

    /** Timestamps, metadata, system lines. 6.1:1 */
    val TextTertiary = Color(0xFF8A8A8A)

    /** Placeholders and keyboard hints — the dimmest tier. 4.9:1 */
    val TextQuaternary = Color(0xFF7A7A7A)

    // -- borders -----------------------------------------------------------

    val BorderSubtle = Color(0x14FFFFFF) // 8%
    val BorderActive = Color(0x29FFFFFF) // 16%

    // -- glass -------------------------------------------------------------

    val GlassWhite = Color(0x0AFFFFFF) // 4%
    val GlassHighlight = Color(0x14FFFFFF) // 8%

    /** Top-edge specular line that gives a glass panel its physical edge. */
    val GlassRimLight = Color(0x33FFFFFF)
    val GlassRimShadow = Color(0x0DFFFFFF)

    /** Scrim used when the device cannot blur — keeps contrast without a shader. */
    val GlassScrim = Color(0xD90A0A0A)

    // -- accents -----------------------------------------------------------
    //
    // Restrained by design. These are for state, not decoration: a live link, a
    // failed send, an SOS tier. They are never used for emphasis or branding.

    /** Barely-there cool white. The default "active" signal. */
    val AccentPrimary = Color(0xFFE8EDF2)

    /** Desaturated green — a link is up. Never a bright terminal green. */
    val AccentSuccess = Color(0xFF6E9E7F)

    /** Muted amber — degraded, queued, retrying. */
    val AccentWarning = Color(0xFFB8935F)

    /** Muted red — reserved for P0/SOS and hard failures. */
    val AccentCritical = Color(0xFFB4685F)

    // -- message bubbles ---------------------------------------------------
    //
    // Kept close to the surface palette — just enough lift to make direction
    // legible without competing with the text or breaking the near-monochrome
    // design rule.

    /** Outbound (sent by this node) — a shade above SurfaceRaised. */
    val BubbleOutbound = Color(0xFF1A2020)

    /** Inbound (received from a peer) — neutral dark surface. */
    val BubbleInbound = Color(0xFF111111)
}
